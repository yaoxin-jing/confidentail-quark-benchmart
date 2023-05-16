use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::runtime};
use kube::{
    api::{
        Api, PostParams, ResourceExt, DeleteParams},
    runtime::wait::{await_condition, conditions::is_pod_running},
    Client,
};
use tokio::time::error;
use std::time::Instant;
use core::result::Result::Ok;

use crate::{WorkloadType, RuntimeType};
use crate::GLOBAL_STATISTICS_KEEPER;
use crate::execute_cmd;
use k8s_openapi::api::core::v1::EnvVar;
use crate::delete_pod;
use crate::parse_quark_log;
use crate::StatisticKeeper;
use crate::get_statistic;
use crate::SANDBOX_START;
use crate::get_log_time_stamp;
use std::io::BufReader;
use std::io::BufRead;
use crate::MyLoger;
use std::fs::File;
use crate::my_loger::reset_log_file;

use tokio::process::Command;
use std::time::{Duration, SystemTime};
use crate::atoi;


const CONTAINER_STATISTIC: &str = "duration";



const POD_TEMPLATE: &str =  "{
    \"apiVersion\": \"v1\",
    \"kind\": \"Pod\",
    \"metadata\": {
        \"name\": \"micro\"
    },
    \"spec\": {
        \"runtimeClassName\": \"quark\",
        \"containers\": [
        {
            \"name\": \"micro\",
            \"image\": \"yaoxinjing/micro-bench:latest\",
            \"ports\": [
            {
                \"containerPort\": 80
            }
            ],
            \"env\": [
            {
                \"name\": \"APPLICATION_NAME\",
                \"value\": \"main\"
            },
            {
                \"name\": \"SECRET_MANAGER_IP\",
                \"value\": \"10.221.117.198:8000\"
            },
            {
                \"name\": \"SHILED_POLICY_PATH\",
                \"value\": \"quark_nginx/nginx_resource/policy\"
            }
            ]
        }
        ]
    }
    }";
    
    
fn is_app_start (runtime_type: &RuntimeType) -> anyhow::Result<u128>{

    let mut sandbox_exit_time: u128 = 0;

    let file = File::open("/var/log/quark/quark.log")?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        
        let line = line?;
        if line.contains(SANDBOX_START) {
            sandbox_exit_time = get_log_time_stamp(&line, runtime_type).unwrap();

            info!("line {:?}", line);
            debug!("{}", sandbox_exit_time);
        }
    }

    if sandbox_exit_time == 0 {
        return Err(anyhow::Error::msg("app start failed"));
    }
    Ok(sandbox_exit_time)
}



async fn wait_for_container_statistic (pod_name: &str, wait_time: u64) ->  anyhow::Result<f64> {

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
            if line.contains(CONTAINER_STATISTIC) {

                let words: Vec<&str> = line.split_whitespace().collect();
                // info!("words {:?}", words);
                let res = atoi::<f64>(words[1])
                        .map_err(|e| anyhow::Error::msg(format!("atoi::<u128>(words[1]) get error {:?}", e)))?;
                
                return Ok(res)
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




pub async fn perf_software_manager (elf_size : i32, loop_time: i32, my_logger: &mut MyLoger<File>, image_name: &str, runtime_type: &RuntimeType) ->  anyhow::Result<()> {

    let pod_name = "micro".to_string();
    
    let client = Client::try_default().await?;
    // Manage pods
    let pods: Api<Pod> = Api::default_namespaced(client);
    let mut p: Pod = serde_json::from_str(POD_TEMPLATE)?;

    p.spec.as_mut().unwrap().containers[0].image =  Some(image_name.to_string());

    let mut statistic_keeper = GLOBAL_STATISTICS_KEEPER.lock().unwrap();
    let log_name = format!("elf-size-{:?}-loop-time-{:?}", elf_size, loop_time);
    reset_log_file(my_logger, &log_name).unwrap();

    info!("elf_size-{:?}-loop_time-{:?}", elf_size, loop_time);

    let mut read_vec_duration = Vec::new();


    
    let mut i = 0;
    loop {
        if i >= loop_time {
            break;
        }  

        error!("============ round {:?} =============", i);
        execute_cmd("sudo rm -f /var/log/quark/quark.log");

        let start = Instant::now();
        match pods.create(&PostParams::default(), &p).await {
            Err(e) => {

                let res = delete_pod(&pod_name).await;
                if res.is_err() {
                    pods.delete(&pod_name, &DeleteParams::default().grace_period(0))
                    .await?
                    .map_left(|pdel| {
                        assert_eq!(pdel.name_any(), pod_name);
                    });
                    error!("create pod got error, {:?}, pods.create got err {:?}", res, e);
                    return Err(anyhow::Error::msg("app start failed"));
                }
            },
            core::result::Result::Ok(_) => {

            }
        }
        // Wait until the pod is running, otherwise we get 500 error.
        let running = await_condition(pods.clone(), &pod_name, is_pod_running());
        match tokio::time::timeout(std::time::Duration::from_secs(60), running).await {
            Ok(_) => {},
            Err(e) => {
                pods.delete(&pod_name, &DeleteParams::default().grace_period(0))
                .await?
                .map_left(|pdel| {
                    assert_eq!(pdel.name_any(), pod_name);
                });
                error!("tokio::time::timeout(std::time::Duration::from_secs(60), running) {:?}", e);
                continue; 
            }
        }


        let read_elf_duration = wait_for_container_statistic(&pod_name, 30).await.unwrap();

        read_vec_duration.push(read_elf_duration);


        match delete_pod(&pod_name).await {
            Err(e) => {
                pods.delete(&pod_name, &DeleteParams::default().grace_period(0))
                .await?
                .map_left(|pdel| {
                    assert_eq!(pdel.name_any(), pod_name);
                });
                error!("delete_pod(&pod_name).await failed {:?}", e);
            },
            core::result::Result::Ok(_) => {
            }
        }
        let deleted = Instant::now();

        let quark_statistic = match parse_quark_log(i, runtime_type) {
            Ok(mut statistic) => {
                let period = (deleted - start).as_secs_f64();
                statistic.total_period_in_s = period;

                info!("quark statistic {:?}", statistic);
                statistic
            },
            Err(e) => {
                error!("parse_quark_log got error {:?}", e);
                continue;
            }
        };
        
        statistic_keeper.pods_statistic.push(quark_statistic);

        
        i = i + 1;
    }

    error!("================== Result ==============================");
    crate::calcaulate_statistic_result(&statistic_keeper, WorkloadType::Micro(read_vec_duration)).unwrap();

    statistic_keeper.nginx_statistics = Vec::new();
    statistic_keeper.redis_statistics = Vec::new();
    statistic_keeper.pods_statistic = Vec::new();

    Ok(())
}