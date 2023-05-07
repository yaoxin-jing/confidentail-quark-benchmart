
#[macro_use]
extern crate lazy_static;

#[macro_use] 
extern crate log;
use futures::lock::MutexGuard;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{
        Api, PostParams, ResourceExt, DeleteParams},
    runtime::wait::{await_condition, conditions::{is_pod_running, is_deleted}},
    Client,
};
use std::{process::{Command, Output}, collections::HashSet};
use anyhow::Context;
use std::time::SystemTime;
use chrono::{offset::Utc, DateTime};
use std::sync::Mutex;
use std::fs::File;
use std::io::{BufReader, BufRead};
use simplelog::*;
use std::str::FromStr;
use std::time::Instant;
use core::result::Result::Ok;

const APPLICATON_START: &str = "application start";
const APPLICATON_EXIT: &str = "application exit";
const REMOTE_ATTESTATION_START: &str = "remote attestation start";
const REMOTE_ATTESTATION_END: &str = "remote attestation finished";
const GENERATE_ATTESTATION_REPORT_START: &str = "generate_evidence start";
const GENERATE_ATTESTATION_REPORT_END: &str = "generate_evidence end";
const SOFT_MEASUREMENT_BEFORE_APP_LAUNCN: &str = "measured_cmp_in_bytes_before_app_launch";
const LIB_MEASURED_BEFORE_APP_LAUNCHED: &str = "shared libs loaded before application lauched";
const LIB_MEASURED_DURING_RUNTIME: &str = "shared libs loaded after application lauched";
const SECRET_INJECTION_START: &str = "secret injection start";
const SECRET_INJECTION_END: &str = "secret injection finished";






lazy_static! {
    static ref Global_Redis_Statistics_Keeper: Mutex<RedisPodStatisticKeeper> = Mutex::new(RedisPodStatisticKeeper::default());
    static ref lib_measured_before_app_launched: Mutex<HashSet<String>> = Mutex::new(HashSet::default());
    static ref lib_measured_during_runtime: Mutex<HashSet<String>> = Mutex::new(HashSet::default());
}

#[derive(Debug, Default)]
struct PodStatistic {
    remote_attestation_and_provision_duration_in_ms: f64,
    get_report_duration_in_ms: f64,
    secret_injection_duration_in_ms: f64,
    app_lanch_duration_in_ms: f64,
    app_running_dution_in_ms:  f64,
    measurement_before_app_launch_in_mb: f64,
    runtime_meausrement_in_mb: f64,
    // How long did the whole pod launch, app running, pod quit take
    total_period_in_s: f64,
}


#[derive(Debug)]
enum WorkloadType {
    Redis,
    Nginx
}

#[derive(Debug)]
enum RuntimeType {
    Baseline,
    Cquark,   
}



#[derive(Debug, Default)]
struct RedisPodStatisticKeeper {
    pods_statistic: Vec<PodStatistic>,
    redis_statistics: Vec<RedisStatistic>,
}

// struct RedisPodsStatistic {
//     remote_attestation_and_provision_duration: Statistic,
//     get_report_duration: Statistic,
//     secret_injection_duration: Statistic,
//     app_lanch_duration: Statistic,
//     app_running_dution:  Statistic,
//     measurement_in_byte_before_app_launch: Statistic,
//     runtime_meausrement_in_bytes: Statistic,
//     // How long did the whole pod launch, app running, pod quit take
//     total_period: Statistic,
//     workload_statistic: RedisGloableStatistic
// }


#[derive(Debug, Default)]
struct OurStatistic {
    mean: f64,
    min: f64,
    max: f64,
    std_deviation: f64,
    median : f64
}


// struct RedisGloableStatistic {
//     duration_mean: u128,
//     duration_min: u128,
//     duration_max: u128,
//     duration_std_deviation: f64,
// }



const PING_INLINE: &str = "PING_INLINE";
const PING_BULK: &str = "PING_BULK";
const SET: &str = "SET";
const GET: &str = "GET";
const INCR: &str = "INCR";
const LPUSH: &str = "LPUSH";
const RPUSH: &str = "RPUSH";
const LPOP: &str = "LPOP";
const RPOP: &str = "RPOP";
const SADD: &str = "SADD";
const HSET: &str = "HSET";
const SPOP: &str = "SPOP";
const MSET: &str = "MSET (10 keys)";

#[derive(Debug, Default)]
struct RedisStatistic {
    ping_inline: f64,
    ping_bulk: f64,
    set: f64,
    get: f64,
    incr:  f64,
    lpush: f64,
    rpush: f64,
    lpop: f64,
    rpop: f64,
    sadd: f64,
    hset: f64,
    spop: f64,
    mset: f64,
    geo_mean: f64,
}


impl RedisStatistic {
    fn cal_geometric_mean (&mut self) -> anyhow::Result<()> {
        let product = self.ping_inline * self.ping_bulk * self.set * self.get * self.incr * self.lpush * self.rpush * self.lpop * self.rpop * self.sadd * self.hset * self.spop * self.mset;
        assert!(product > 0.0);
        let root = 1.0 / 13.0;
        self.geo_mean = f64::powf(product, root);
        Ok(())
    }
}



fn collect_redis_statistics(redis_output : Output) -> anyhow::Result<RedisStatistic> {

    let res = String::from_utf8_lossy(&redis_output.stdout);
    let mut redis_statistic = RedisStatistic::default();

    let mut redis_statistic_inited = false;
    for line in res.lines() {
        let values: Vec<&str> = line.split(",").collect();
        let key = &values[0][1..values[0].len()-1];
        let value = values[1][1..values[1].len()-1].parse::<f64>().unwrap();

        debug!("valuse {:?} key {:?}, value {:?}", values, key, value);

        if key.eq(PING_INLINE) {
            redis_statistic.ping_inline= value;
            redis_statistic_inited = true;
        } else if key.eq(PING_BULK) {
            redis_statistic.ping_bulk= value;
        } else if key.eq(SET) {
            redis_statistic.set= value;
        } else if key.eq(GET) {
            redis_statistic.get= value;
        } else if key.eq(INCR) {
            redis_statistic.incr= value;
        } else if key.eq(LPUSH) {
            redis_statistic.lpush= value;
        } else if key.eq(RPUSH) {
            redis_statistic.rpush= value;
        } else if key.eq(LPOP) {
            redis_statistic.lpop= value;
        } else if key.eq(RPOP) {
            redis_statistic.rpop= value;
        } else if key.eq(SADD) {
            redis_statistic.sadd= value;
        } else if key.eq(HSET) {
            redis_statistic.hset= value;
        } else if key.eq(SPOP) {
            redis_statistic.spop= value;
        } else if key.eq(MSET) {
            redis_statistic.mset= value;
        }
    }

    if !redis_statistic_inited {
        error!("collect_redis_statistics failed  redis output {:?}", redis_output);
        return Err(anyhow::Error::msg("collect_redis_statistics failed"));   
    }

    redis_statistic.cal_geometric_mean().unwrap();

    // info!("{:?}", redis_statistic);
    Ok(redis_statistic)

}


async fn run_redis_benchmark(pod_ip: String) ->  anyhow::Result<Output>{

    // Compile code. | grep 'per second'
    let cmd = format!("redis-benchmark -h {} -p 6379 -n 100000 -c 20 --csv", pod_ip);

    let output = Command::new("bash")
    .args([
        "-c",
        &cmd
    ])
    .output()
    .map_err(|e| anyhow::Error::msg(format!("run_redis_benchmark failed to run redus benchmark, get error {:?}", e)))?;

    // println!("status: {}", output.status);
    let res = String::from_utf8_lossy(&output.stdout);

    debug!("stdout: {}", res);
    // println!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    Ok(output)
}



async fn delete_pod(pod_name: &str) ->  anyhow::Result<()>{

    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::default_namespaced(client);
    let pod_uid = pods.get_metadata(&pod_name).await?.uid().expect("failed to get pod uid");
    pods.delete(&pod_name, &DeleteParams::default())
    .await?
    .map_left(|pdel| {
        assert_eq!(pdel.name_any(), pod_name);
    });

    let deleted = await_condition(pods.clone(), &pod_name, is_deleted(&pod_uid));
    let _ = tokio::time::timeout(std::time::Duration::from_secs(30), deleted).await?;

    Ok(())
}


use statrs::distribution::Uniform;
use statrs::statistics::Median;
use statrs::statistics::Statistics;

fn get_statistic(data: &[f64]) -> anyhow::Result<OurStatistic> {


    let n = Uniform::new(data.min(), data.max()).unwrap();

    let s = OurStatistic {
        std_deviation: data.std_dev(),
        mean: data.mean(),
        min: data.min(),
        max: data.max(),
        median: n.median(), 
    };

    Ok(s)
}

fn calculate_redis_result(statistic_keeper: &std::sync::MutexGuard<RedisPodStatisticKeeper>) {


    let mut ping_inline: Vec<f64> = Vec::new();
    let mut ping_bulk: Vec<f64> = Vec::new();
    let mut set: Vec<f64> = Vec::new();

    let mut get: Vec<f64> = Vec::new();
    let mut incr: Vec<f64> = Vec::new();

    let mut lpush: Vec<f64> = Vec::new();
    let mut rpush: Vec<f64> = Vec::new();
    let mut lpop: Vec<f64> = Vec::new();

    let mut rpop: Vec<f64> = Vec::new();
    let mut sadd: Vec<f64> = Vec::new();
    let mut hset: Vec<f64> = Vec::new();

    let mut spop: Vec<f64> = Vec::new();
    let mut mset: Vec<f64> = Vec::new();
    let mut geo_mean: Vec<f64> = Vec::new();


    
    for item in &statistic_keeper.redis_statistics {
        ping_inline.push(item.ping_inline);
        ping_bulk.push(item.ping_bulk);
        set.push(item.set);

        get.push(item.get);
        incr.push(item.incr);

        lpush.push(item.lpush);
        rpush.push(item.rpush);
        lpop.push(item.lpop);

        
        rpop.push(item.rpop);
        sadd.push(item.sadd);
        hset.push(item.hset);

        spop.push(item.spop);
        mset.push(item.mset);
        geo_mean.push(item.geo_mean);
    }

    let ping_inline_statistic = get_statistic(&ping_inline).unwrap();
    let ping_bulk_statistic = get_statistic(&ping_bulk).unwrap();
    let set_statistic = get_statistic(&set).unwrap();

    let get_s = get_statistic(&get).unwrap();
    let incr_statistic = get_statistic(&incr).unwrap();

    let lpush_statistic = get_statistic(&lpush).unwrap();
    let rpush_statistic = get_statistic(&rpush).unwrap();
    let lpop_statistic = get_statistic(&lpop).unwrap();

    let rpop_statistic = get_statistic(&rpop).unwrap();
    let sadd_statistic = get_statistic(&sadd).unwrap();
    let hset_statistic = get_statistic(&hset).unwrap();

    let spop_statistic = get_statistic(&spop).unwrap();
    let mset_statistic = get_statistic(&mset).unwrap();
    let geo_mean_statistic = get_statistic(&geo_mean).unwrap();


    info!("ping_inline_statistic : {:?}", ping_inline_statistic);
    info!("ping_bulk_statistic : {:?}", ping_bulk_statistic);
    info!("set_statistic : {:?}", set_statistic);

    info!("get_s : {:?}", get_s);
    info!("incr_statistic : {:?}", incr_statistic);

    info!("lpush_statistic : {:?}", lpush_statistic);
    info!("rpush_statistic : {:?}", rpush_statistic);
    info!("lpop_statistic : {:?}", lpop_statistic);

    info!("rpop_statistic : {:?}", rpop_statistic);
    info!("sadd_statistic : {:?}", sadd_statistic);
    info!("hset_statistic : {:?}", hset_statistic);

    info!("spop_statistic : {:?}", spop_statistic);
    info!("mset_statistic : {:?}", mset_statistic);
    info!("geo_mean_statistic : {:?}", geo_mean_statistic);

}

fn calcaulate_statistic_result(statistic_keeper: &std::sync::MutexGuard<RedisPodStatisticKeeper>, workload_type: WorkloadType) -> anyhow::Result<()> {

    let mut remote_attestation_and_provision_duration_in_ms: Vec<f64> = Vec::new();
    let mut get_report_duration_in_ms: Vec<f64> = Vec::new();
    let mut secret_injection_duration_in_ms: Vec<f64> = Vec::new();
    let mut app_lanch_duration_in_ms: Vec<f64> = Vec::new();
    let mut app_running_dution_in_ms: Vec<f64> = Vec::new();
    let mut measurement_before_app_launch_in_mb: Vec<f64> = Vec::new();
    let mut runtime_meausrement_in_mb: Vec<f64> = Vec::new();
    let mut total_period_in_s: Vec<f64> = Vec::new();

    for item in &statistic_keeper.pods_statistic {
        remote_attestation_and_provision_duration_in_ms.push(item.remote_attestation_and_provision_duration_in_ms);
        get_report_duration_in_ms.push(item.get_report_duration_in_ms);
        secret_injection_duration_in_ms.push(item.secret_injection_duration_in_ms);
        app_lanch_duration_in_ms.push(item.app_lanch_duration_in_ms);
        app_running_dution_in_ms.push(item.app_running_dution_in_ms);
        measurement_before_app_launch_in_mb.push(item.measurement_before_app_launch_in_mb);
        runtime_meausrement_in_mb.push(item.runtime_meausrement_in_mb);
        total_period_in_s.push(item.total_period_in_s);
    }

    let remote_attestation_and_provision_statistic_in_ms = get_statistic(&remote_attestation_and_provision_duration_in_ms).unwrap();
    let get_report_duration_in_ms_statistic = get_statistic(&get_report_duration_in_ms).unwrap();
    let secret_injection_duration_in_ms_statistic = get_statistic(&secret_injection_duration_in_ms).unwrap();
    let app_lanch_duration_in_ms_statistic = get_statistic(&app_lanch_duration_in_ms).unwrap();
    let app_running_dution_in_ms_statistic = get_statistic(&app_running_dution_in_ms).unwrap();
    let measurement_before_app_launch_in_mb_statistic = get_statistic(&measurement_before_app_launch_in_mb).unwrap();
    let runtime_meausrement_in_mb_statistic = get_statistic(&runtime_meausrement_in_mb).unwrap();
    let total_period_in_s_statistic = get_statistic(&total_period_in_s).unwrap();


    info!("remote_attestation_and_provision_statistic_in_ms: {:?}", remote_attestation_and_provision_statistic_in_ms);
    info!("get_report_duration_in_ms_statistic: {:?}", get_report_duration_in_ms_statistic);
    info!("secret_injection_duration_in_ms_statistic: {:?}", secret_injection_duration_in_ms_statistic);
    info!("app_lanch_duration_in_ms_statistic: {:?}", app_lanch_duration_in_ms_statistic);
    info!("app_running_dution_in_ms_statistic: {:?}", app_running_dution_in_ms_statistic);
    info!("measurement_before_app_launch_in_mb_statistic: {:?}", measurement_before_app_launch_in_mb_statistic);
    info!("runtime_meausrement_in_mb_statistic: {:?}", runtime_meausrement_in_mb_statistic);
    info!("total_period_in_s_statistic: {:?}", total_period_in_s_statistic);

    match workload_type {
        WorkloadType::Redis => {
            calculate_redis_result(statistic_keeper);
        },
        WorkloadType::Nginx => {

        }
    }
    Ok(())
}


async fn test_app_lauch_time(loop_times: i32, pod_name: String, file_path: std::path::PathBuf, workload_type: WorkloadType) ->  anyhow::Result<i32>{

    let client = Client::try_default().await?;
    let yaml = std::fs::read_to_string(&file_path).with_context(|| format!("Failed to read {}", file_path.display()))?;
    let p: Pod = serde_yaml::from_str(&yaml)?;
    let pods: Api<Pod> = Api::default_namespaced(client);



    let mut statistic_keeper = Global_Redis_Statistics_Keeper.lock().unwrap();

    let mut i = 0;
    while i < loop_times {

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
        let _ = tokio::time::timeout(std::time::Duration::from_secs(60), running).await
            .map_err(|e| anyhow::Error::msg(format!("tokio::time::timeout(std::time::Duration::from_secs(60), running), get error {:?}", e)))?;


        let pod_cluster_ip = pods.get(&pod_name).await
            .map_err(|e| anyhow::Error::msg(format!("ods.get(&pod_name).await get error {:?}", e)))?
                .status.unwrap().pod_ip.unwrap();
        debug!("pod ip {}", pod_cluster_ip);


        let redis_output = run_redis_benchmark(pod_cluster_ip).await;


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

        if redis_output.is_err() {
            i = i + 1;
            continue;
        }

        let redis_statistic = match collect_redis_statistics(redis_output.unwrap()) {
            Ok(statistic) => {

                info!("redis statistic {:?}", statistic);
                statistic
            },
            Err(e) => {
                i = i + 1;
                error!("collect_redis_statistics got error {:?}", e);
                continue;
            }
        };

        let quark_statistic = match parse_quark_log() {
            Ok(mut statistic) => {
                let period = (deleted - start).as_secs_f64();
                statistic.total_period_in_s = period;

                info!("quark statistic {:?}", statistic);
                statistic
            },
            Err(e) => {
                i = i + 1;
                error!("parse_quark_log got error {:?}", e);
                continue;
            }
        };

        statistic_keeper.pods_statistic.push(quark_statistic);
        statistic_keeper.redis_statistics.push(redis_statistic);


        i = i + 1;
    }

    calcaulate_statistic_result(&statistic_keeper, workload_type).unwrap();

    Ok(i)    
}



fn execute_cmd(cmd: &str) {

     Command::new("bash")
    .args([
        "-c",
        cmd
    ])
    .output()
    .expect(&format!("failed to execute cmd {}", cmd));

}




fn is_app_started() -> anyhow::Result<u128>{

    let mut app_start_time: u128 = 0;

    let file = File::open("/var/log/quark/quark.log")?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        
        let line = line?;

        if line.contains(APPLICATON_START) {
            app_start_time = get_log_time_stamp(&line).unwrap();

            debug!("{}", app_start_time);
        }
    }

    if app_start_time == 0 {
        return Err(anyhow::Error::msg("app start failed"));
    }

    Ok(app_start_time)


}


fn atoi<F: FromStr>(input: &str) -> Result<F, <F as FromStr>::Err> {
    let i = input.find(|c: char| !c.is_numeric()).unwrap_or_else(|| input.len());
    input[..i].parse::<F>()
}


fn get_log_time_stamp(str: &str) -> anyhow::Result<u128> {
    let words: Vec<&str> = str.split_whitespace().collect();
    atoi::<u128>(words[3])
    .map_err(|e| anyhow::Error::msg(format!("get_log_time_stamp get error {:?}", e)))
}

fn parse_quark_log() -> anyhow::Result<PodStatistic> {

    let app_start_time_in_ns = is_app_started()?;
    let mut app_exit_time_in_ns= 0;
    let mut runtime_meausrement_in_bytes = 0;
    let mut remote_attestation_start_in_ns = 0;
    let mut remote_attestation_end_in_ns = 0;
    let mut generate_attestation_report_start_in_ns = 0;
    let mut generate_attestation_report_end_in_ns = 0;
    let mut measurement_in_byte_before_app_launch = 0;

    let mut secret_injection_start_in_ns = 0;
    let mut secret_injection_end_in_ns = 0;

    let file = File::open("/var/log/quark/quark.log")?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        
        let line = line?;

        if line.contains(APPLICATON_EXIT) {
            app_exit_time_in_ns = get_log_time_stamp(&line).unwrap();
            let words: Vec<&str> = line.split_whitespace().collect();
            runtime_meausrement_in_bytes = words[10].parse::<u128>().unwrap();
            debug!("app_exit_time {:?} runtime_meausrement_in_bytes {:?}", app_exit_time_in_ns, runtime_meausrement_in_bytes);
        } else if line.contains(REMOTE_ATTESTATION_START) {
            remote_attestation_start_in_ns = get_log_time_stamp(&line).unwrap();
            debug!("REMOTE_ATTESTATION_START {}", remote_attestation_start_in_ns);
        } else if line.contains(REMOTE_ATTESTATION_END) {
            remote_attestation_end_in_ns = get_log_time_stamp(&line).unwrap();

            debug!("REMOTE_ATTESTATION_END {}", remote_attestation_end_in_ns);
        } else if line.contains(GENERATE_ATTESTATION_REPORT_START) {
            generate_attestation_report_start_in_ns = get_log_time_stamp(&line).unwrap();
            debug!("GENERATE_ATTESTATION_REPORT_START {}", generate_attestation_report_start_in_ns);

        } else if line.contains(GENERATE_ATTESTATION_REPORT_END) {
            generate_attestation_report_end_in_ns = get_log_time_stamp(&line).unwrap();
            debug!("GENERATE_ATTESTATION_REPORT_END {}", generate_attestation_report_end_in_ns);
        }  else if line.contains(SOFT_MEASUREMENT_BEFORE_APP_LAUNCN) {
            let words: Vec<&str> = line.split_whitespace().collect();
            measurement_in_byte_before_app_launch = words[4].parse::<u128>().unwrap();
            // runtime_meausrement_in_bytes = words[11].parse::<u64>().unwrap();
            debug!("SOFT_MEASUREMENT_BEFORE_APP_LAUNCN {:?}", measurement_in_byte_before_app_launch);
        } else if line.contains(LIB_MEASURED_BEFORE_APP_LAUNCHED) {
            let words: Vec<&str> = line.split_whitespace().collect();

            let lib_path: Vec<&str> = words[10].split("/").collect();
            let lib_name: String = lib_path[lib_path.len() - 1].strip_suffix("\"").unwrap().to_string();

            let mut libs = lib_measured_before_app_launched.lock().unwrap();
            if !libs.contains(&lib_name) {
                libs.insert(lib_name);
            }
            debug!("LIB_MEASURED_BEFORE_APP_LAUNCHED {:?}", libs);
        } else if line.contains(LIB_MEASURED_DURING_RUNTIME) {
            let words: Vec<&str> = line.split_whitespace().collect();

            let lib_path: Vec<&str> = words[10].split("/").collect();
            let lib_name: String = lib_path[lib_path.len() - 1].strip_suffix("\"").unwrap().to_string();

            let mut libs = lib_measured_during_runtime.lock().unwrap();
            if !libs.contains(&lib_name) {
                libs.insert(lib_name);
            }
            // runtime_meausrement_in_bytes = words[11].parse::<u64>().unwrap();
            debug!("LIB_MEASURED_DURING_RUNTIME {:?}", libs);
        } else if line.contains(SECRET_INJECTION_START) {
            secret_injection_start_in_ns = get_log_time_stamp(&line).unwrap();
            debug!("SECRET_INJECTION_START {}", secret_injection_start_in_ns);

        }  else if line.contains(SECRET_INJECTION_END) {
            secret_injection_end_in_ns = get_log_time_stamp(&line).unwrap();
            debug!("SECRET_INJECTION_END {}", secret_injection_end_in_ns);

        } 
    }

    let secret_injection_duration = secret_injection_end_in_ns - secret_injection_start_in_ns ;
    let get_report_duration = generate_attestation_report_end_in_ns - generate_attestation_report_start_in_ns; 
    let remote_attestation_and_provision_duration = remote_attestation_end_in_ns - remote_attestation_start_in_ns - get_report_duration;
    let app_running_time = app_exit_time_in_ns - app_start_time_in_ns;
    // time used by  base line component to set up env for contianer  + software measurement overhead
    let app_lanch_time = app_start_time_in_ns - remote_attestation_and_provision_duration - get_report_duration;


    info!("app_lanch_time {}, app_running_time {}, remote_attestation_and_provision_duration {}, get_report_duration {}, secret_injection_duration {} measurement_in_byte_before_app_launch {} runtime_meausrement_in_bytes {}", 
        app_lanch_time, app_running_time, remote_attestation_and_provision_duration, get_report_duration, secret_injection_duration, measurement_in_byte_before_app_launch, runtime_meausrement_in_bytes);


    let statistic = PodStatistic {
        remote_attestation_and_provision_duration_in_ms: (remote_attestation_and_provision_duration as f64 / 1000000.0),
        get_report_duration_in_ms: (get_report_duration as f64) / 1000000.0,
        secret_injection_duration_in_ms: (secret_injection_duration as f64) / 1000000.0,
        app_lanch_duration_in_ms: (app_lanch_time as f64) / 1000000.0,
        app_running_dution_in_ms: (app_running_time as f64) / 1000000.0,
        measurement_before_app_launch_in_mb: (measurement_in_byte_before_app_launch as f64) / 1024.0,
        runtime_meausrement_in_mb: (runtime_meausrement_in_bytes as f64) / 1024.0,
        ..Default::default()
    };

    Ok(statistic)
}





fn setup(runtime_type: RuntimeType, workload_type: WorkloadType) -> anyhow::Result<()> {

    let current_time = SystemTime::now();
    let datetime: DateTime<Utc> = current_time.into();
    let dir_name = format!("benchmark-{}-{:?}-{:?}", datetime.format("%d-%m-%Y-%T"), runtime_type, workload_type);
    let cmd = format!("mkdir -p {}", dir_name);

    execute_cmd(&cmd);
    execute_cmd("sudo rm -f /var/log/quark/quark.log");

    let mut current_path = std::env::current_dir().unwrap();
    current_path.push(&dir_name);
    assert!(std::env::set_current_dir(&current_path).is_ok());

    let time_format = simplelog::format_description!("[year]:[month]:[day]:[hour]:[minute]:[second].[subsecond]");

    let config = ConfigBuilder::new()
    .set_time_format_custom(time_format)
    .build();

    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Info, config.clone(), TerminalMode::Mixed, ColorChoice::Auto),
            WriteLogger::new(LevelFilter::Info, config, File::create("benchmark.log").unwrap()),
        ]
    ).unwrap();

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    // tracing_subscriber::fmt::init();
    setup(RuntimeType::Cquark, WorkloadType::Redis).unwrap();


    // parse_quark_log();
    
    let path = std::path::PathBuf::from("/home/yaoxin/demo/benchmark/redis.yaml");
    let res = test_app_lauch_time(5, "redis".to_string(), path, WorkloadType::Redis).await?;
    assert!(res == 5);
    Ok(())
}