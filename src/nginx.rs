
use std::process::Output;
use tokio::process::Command;
use std::time::{Duration, SystemTime};
use crate::StatisticKeeper;
use crate::get_statistic;

const NGINX_STARTED: &str = "start worker processes";
const REQUEST_PER_S: &str = "Requests per second:";



#[derive(Debug, Default)]
pub struct NginxStatistic {
    request_per_second: f64,
}


pub fn calculate_nginx_result(statistic_keeper: &std::sync::MutexGuard<StatisticKeeper>) {


    let mut reqs_per_s: Vec<f64> = Vec::new();
    for item in &statistic_keeper.nginx_statistics {
        reqs_per_s.push(item.request_per_second);
    }

    let requst_per_ss: crate::OurStatistic = get_statistic(&reqs_per_s).unwrap();
    info!("nginx statistics : {:?}", requst_per_ss);

}


pub fn collect_nginx_statistics(redis_output : Output) -> anyhow::Result<NginxStatistic> {

    let res = String::from_utf8_lossy(&redis_output.stdout);
    let mut nginx_statistic = NginxStatistic::default();

    let mut nginx_statistic_inited = false;
    for line in res.lines() {
        if line.contains(REQUEST_PER_S) {
            let words: Vec<&str> = line.split_whitespace().collect();

            info!("collect_nginx_statistics {:?}", words);
            nginx_statistic.request_per_second = words[3].parse::<f64>().unwrap();
            nginx_statistic_inited = true;
        }
    }

    if !nginx_statistic_inited {
        error!("collect_redis_statistics failed  redis output {:?}", redis_output);
        return Err(anyhow::Error::msg("collect_redis_statistics failed"));   
    }

    // info!("{:?}", redis_statistic);
    Ok(nginx_statistic)

}




async fn wait_for_nginx_to_run (pod_name: &str, wait_time: u64) ->  anyhow::Result<()> {

    let cmd = format!("kubectl logs {}", pod_name);

    let seconds = Duration::from_secs(wait_time);
    let start = SystemTime::now();
    loop {

        let output = Command::new("sh")
        .args([
            "-c",
            &cmd
        ])
        .output()
        .await.map_err(|e| anyhow::Error::msg(format!("run_nginx_benchmark failed to run nginx benchmark, get error {:?}", e)))?;
        
        
        let res = String::from_utf8_lossy(&output.stdout);
        debug!("log stdout: {}", res);

        for line in res.lines() {
            if line.contains(NGINX_STARTED) {
                return Ok(())
            }
        }

        match start.elapsed() {
            Ok(elapsed) if elapsed > seconds => {
                return Err(anyhow::Error::msg(format!("wait_for_nginx_to_run {} elapsed", wait_time)));
            }
            _ => (),
        }

    }
}


pub async fn run_nginx_benchmark(pod_ip: &str, pod_name: &str) ->  anyhow::Result<Output> {

    wait_for_nginx_to_run(pod_name, 5).await?;

    let cmd = format!("ab -n 1000 -c 10 http://{}:80/index.html", pod_ip);
    let output = Command::new("sh")
    .args([
        "-c",
        &cmd
    ])
    .output()
    .await.map_err(|e| anyhow::Error::msg(format!("run_nginx_benchmark failed to run nginx benchmark, get error {:?}", e)))?;


    let res = String::from_utf8_lossy(&output.stdout);
    info!("ab stdout: {}", res);

    return Ok(output);
}
