
mod redis;
mod nginx;
mod kbs_https_clinet;


#[macro_use]
extern crate lazy_static;

#[macro_use] 
extern crate log;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{
        Api, PostParams, ResourceExt, DeleteParams},
    runtime::wait::{await_condition, conditions::{is_pod_running, is_deleted}},
    Client,
};
use std::{process::Command, collections::HashSet};
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
use statrs::distribution::Uniform;
use statrs::statistics::Median;
use statrs::statistics::Statistics;

use crate::nginx::NginxStatistic;
use crate::nginx::collect_nginx_statistics;
use crate::nginx::calculate_nginx_result;

use crate::redis::{RedisStatistic, calculate_redis_result, run_redis_benchmark, collect_redis_statistics};
use crate::nginx::run_nginx_benchmark;
use crate::kbs_https_clinet::perf_https_attestation_and_provisioning_client;

const SANDBOX_START: &str = "sandbox start";
const SANDBOX_EXIT: &str = "sandbox exit";
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
    pub static ref GLOBAL_STATISTICS_KEEPER: Mutex<StatisticKeeper> = Mutex::new(StatisticKeeper::default());
    static ref LIB_MEASURED_BEFORE_APP_LAUNCHED_KEEPER: Mutex<HashSet<String>> = Mutex::new(HashSet::default());
    static ref LIB_MEASURED_DURING_RUNTIME_KEEPER: Mutex<HashSet<String>> = Mutex::new(HashSet::default());
}

#[derive(Debug, Default)]
struct PodStatistic {
    remote_attestation_and_provision_duration_in_ms: f64,
    get_report_duration_in_ms: f64,
    secret_injection_duration_in_ms: f64,
    pure_app_lanch_duration_in_ms: f64,
    total_app_lanch_time_in_ms: f64,
    app_running_dution_in_ms:  f64,
    sandbox_exit_duration_in_ms: f64,
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

#[derive(Debug, PartialEq)]
pub enum RuntimeType {
    Baseline,
    Cquark,   
}



#[derive(Debug, Default)]
pub struct StatisticKeeper {
    pods_statistic: Vec<PodStatistic>,
    redis_statistics: Vec<RedisStatistic>,
    nginx_statistics: Vec<NginxStatistic>
}


#[derive(Debug, Default)]
struct OurStatistic {
    mean: f64,
    min: f64,
    max: f64,
    std_deviation: f64,
    median : f64
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



fn calcaulate_statistic_result(statistic_keeper: &std::sync::MutexGuard<StatisticKeeper>, workload_type: WorkloadType) -> anyhow::Result<()> {

    let mut remote_attestation_and_provision_duration_in_ms: Vec<f64> = Vec::new();
    let mut get_report_duration_in_ms: Vec<f64> = Vec::new();
    let mut secret_injection_duration_in_ms: Vec<f64> = Vec::new();

    let mut pure_app_lanch_duration_in_ms: Vec<f64> = Vec::new();
    let mut total_app_lanch_time_in_ms:Vec<f64> = Vec::new();
    let mut app_running_dution_in_ms: Vec<f64> = Vec::new();
    let mut sandbox_exit_duration_in_ms: Vec<f64> = Vec::new();

    let mut measurement_before_app_launch_in_mb: Vec<f64> = Vec::new();
    let mut runtime_meausrement_in_mb: Vec<f64> = Vec::new();



    let mut total_period_in_s: Vec<f64> = Vec::new();

    for item in &statistic_keeper.pods_statistic {
        remote_attestation_and_provision_duration_in_ms.push(item.remote_attestation_and_provision_duration_in_ms);
        get_report_duration_in_ms.push(item.get_report_duration_in_ms);
        secret_injection_duration_in_ms.push(item.secret_injection_duration_in_ms);

        pure_app_lanch_duration_in_ms.push(item.pure_app_lanch_duration_in_ms);
        total_app_lanch_time_in_ms.push(item.total_app_lanch_time_in_ms);
        app_running_dution_in_ms.push(item.app_running_dution_in_ms);
        sandbox_exit_duration_in_ms.push(item.sandbox_exit_duration_in_ms);

        measurement_before_app_launch_in_mb.push(item.measurement_before_app_launch_in_mb);
        runtime_meausrement_in_mb.push(item.runtime_meausrement_in_mb);
        total_period_in_s.push(item.total_period_in_s);
    }

    let remote_attestation_and_provision_statistic_in_ms = get_statistic(&remote_attestation_and_provision_duration_in_ms).unwrap();
    let get_report_duration_in_ms_statistic = get_statistic(&get_report_duration_in_ms).unwrap();
    let secret_injection_duration_in_ms_statistic = get_statistic(&secret_injection_duration_in_ms).unwrap();

    let pure_app_lanch_duration_in_ms_statistic = get_statistic(&pure_app_lanch_duration_in_ms).unwrap();
    let total_app_lanch_time_in_ms_statistic = get_statistic(&total_app_lanch_time_in_ms).unwrap();

    let app_running_dution_in_ms_statistic = get_statistic(&app_running_dution_in_ms).unwrap();
    let sandbox_exit_duration_in_ms_statistic = get_statistic(&sandbox_exit_duration_in_ms).unwrap();

    let measurement_before_app_launch_in_mb_statistic = get_statistic(&measurement_before_app_launch_in_mb).unwrap();
    let runtime_meausrement_in_mb_statistic = get_statistic(&runtime_meausrement_in_mb).unwrap();
    let total_period_in_s_statistic = get_statistic(&total_period_in_s).unwrap();


    info!("remote_attestation_and_provision_statistic_in_ms: {:?}", remote_attestation_and_provision_statistic_in_ms);
    info!("get_report_duration_in_ms_statistic: {:?}", get_report_duration_in_ms_statistic);
    info!("secret_injection_duration_in_ms_statistic: {:?}", secret_injection_duration_in_ms_statistic);

    info!("pure_app_lanch_duration_in_ms_statistic: {:?}", pure_app_lanch_duration_in_ms_statistic);
    info!("total_app_lanch_time_in_ms_statistic: {:?}", total_app_lanch_time_in_ms_statistic);
    info!("app_running_dution_in_ms_statistic: {:?}", app_running_dution_in_ms_statistic);
    info!("sandbox_exit_duration_in_ms_statistic: {:?}", sandbox_exit_duration_in_ms_statistic);

    info!("measurement_before_app_launch_in_mb_statistic: {:?}", measurement_before_app_launch_in_mb_statistic);
    info!("runtime_meausrement_in_mb_statistic: {:?}", runtime_meausrement_in_mb_statistic);
    info!("total_period_in_s_statistic: {:?}", total_period_in_s_statistic);

    match workload_type {
        WorkloadType::Redis => {
            calculate_redis_result(statistic_keeper);
        },
        WorkloadType::Nginx => {

            calculate_nginx_result(statistic_keeper)
        }
    }
    Ok(())
}


async fn test_app_lauch_time(loop_times: i32, pod_name: String, file_path: std::path::PathBuf, workload_type: WorkloadType, runtime_type: RuntimeType) ->  anyhow::Result<i32>{

    let client = Client::try_default().await?;
    let yaml = std::fs::read_to_string(&file_path).with_context(|| format!("Failed to read {}", file_path.display()))?;

    info!("yaml {}", yaml);
    let p: Pod = serde_yaml::from_str(&yaml)?;
    let pods: Api<Pod> = Api::default_namespaced(client);



    let mut statistic_keeper = GLOBAL_STATISTICS_KEEPER.lock().unwrap();

    let mut i = 0;
    while i < loop_times {

        error!("====round {}===========", i);
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


        let pod_cluster_ip = pods.get(&pod_name).await
            .map_err(|e| anyhow::Error::msg(format!("ods.get(&pod_name).await get error {:?}", e)))?
                .status.unwrap().pod_ip.unwrap();
        info!("pod ip {}", pod_cluster_ip);


        let output = match workload_type {
            WorkloadType::Redis => run_redis_benchmark(pod_cluster_ip).await,
            WorkloadType::Nginx => run_nginx_benchmark(&pod_cluster_ip, &pod_name).await,
        };


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

        if output.is_err() {
            i = i + 1;
            continue;
        }

        let mut redis_statistic = RedisStatistic::default();
        let mut nginx_statistic = NginxStatistic::default();
        
        match workload_type {
            WorkloadType::Redis =>  {
                redis_statistic = match collect_redis_statistics(output.unwrap()) {
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
            }
            WorkloadType::Nginx => {
                // ToDo
                nginx_statistic = match collect_nginx_statistics(output.unwrap()) {
                    Ok(statistic) => {
        
                        info!("ngins statistic {:?}", statistic);
                        statistic
                    },
                    Err(e) => {
                        i = i + 1;
                        error!("collect_nginx_statistics got error {:?}", e);
                        continue;
                    }
                };
            }
        };

        let quark_statistic = match parse_quark_log(i, &runtime_type) {
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

        match workload_type {
            WorkloadType::Redis =>  {
                statistic_keeper.redis_statistics.push(redis_statistic);
            }
            WorkloadType::Nginx => {
                statistic_keeper.nginx_statistics.push(nginx_statistic);
            }
        };
        
        statistic_keeper.pods_statistic.push(quark_statistic);

        i = i + 1;
    }

    error!("================== Result ==============================");

    {
        info!("libaries loaded before application is lauched {:?}", *LIB_MEASURED_BEFORE_APP_LAUNCHED_KEEPER.lock().unwrap());
        info!("libaries loaded during runtime {:?}", *LIB_MEASURED_DURING_RUNTIME_KEEPER.lock().unwrap());
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

fn is_application_exit(runtime_type: &RuntimeType) -> anyhow::Result<u128>{

    let mut app_start_exit: u128 = 0;

    let file = File::open("/var/log/quark/quark.log")?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        
        let line = line?;
        if line.contains(APPLICATON_EXIT) {
            app_start_exit = get_log_time_stamp(&line, runtime_type).unwrap();

            info!("line {:?}", line);
            debug!("{}", app_start_exit);
        }
    }

    if app_start_exit == 0 {
        return Err(anyhow::Error::msg("app start failed"));
    }

    Ok(app_start_exit)


}


fn is_sandbox_exit (runtime_type: &RuntimeType) -> anyhow::Result<u128>{

    let mut sandbox_exit_time: u128 = 0;

    let file = File::open("/var/log/quark/quark.log")?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        
        let line = line?;
        if line.contains(SANDBOX_EXIT) {
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


fn atoi<F: FromStr>(input: &str) -> Result<F, <F as FromStr>::Err> {
    let i = input.find(|c: char| !c.is_numeric()).unwrap_or_else(|| input.len());
    input[..i].parse::<F>()
}


fn get_log_time_stamp(str: &str, runtime_type: &RuntimeType) -> anyhow::Result<u128> {
    let words: Vec<&str> = str.split_whitespace().collect();
    match runtime_type {
        RuntimeType::Baseline => {
            atoi::<u128>(words[2])
            .map_err(|e| anyhow::Error::msg(format!("get_log_time_stamp get error {:?}", e)))
        },
        RuntimeType::Cquark =>  {
            atoi::<u128>(words[3])
            .map_err(|e| anyhow::Error::msg(format!("get_log_time_stamp get error {:?}", e)))
        }
    }

}

fn parse_quark_log(round: i32, runtime_type: &RuntimeType) -> anyhow::Result<PodStatistic> {

    let mut app_start_time_in_ns = 0;
    let app_exit_time_in_ns= is_application_exit(runtime_type)?;
    let mut runtime_meausrement_in_bytes = 0;
    let mut remote_attestation_start_in_ns = 0;
    let mut remote_attestation_end_in_ns = 0;
    let mut generate_attestation_report_start_in_ns = 0;
    let mut generate_attestation_report_end_in_ns = 0;
    let mut measurement_in_byte_before_app_launch = 0;

    let mut secret_injection_start_in_ns = 0;
    let mut secret_injection_end_in_ns = 0;

    let mut sandbox_start_time_in_ns = 0;
    let sandbox_exit_time_in_ns = is_sandbox_exit(runtime_type)?;

    let file = File::open("/var/log/quark/quark.log")?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        if line.contains(SANDBOX_START) {
            sandbox_start_time_in_ns = get_log_time_stamp(&line, runtime_type).unwrap();
            debug!("sandbox_start_time_in_ns {:?}", sandbox_start_time_in_ns);
        }  else if line.contains(APPLICATON_START) {
            app_start_time_in_ns = get_log_time_stamp(&line, runtime_type).unwrap();
            debug!("app_start_time_in_ns {:?} runtime_meausrement_in_bytes {:?}", app_exit_time_in_ns, runtime_meausrement_in_bytes);
        } else if line.contains(APPLICATON_EXIT) {
            if runtime_type == &RuntimeType::Cquark {
                let words: Vec<&str> = line.split_whitespace().collect();
                runtime_meausrement_in_bytes = words[10].parse::<u128>().unwrap();
            }
            debug!("app_start_time_in_ns {:?} runtime_meausrement_in_bytes {:?}", app_exit_time_in_ns, runtime_meausrement_in_bytes);
        } else if line.contains(REMOTE_ATTESTATION_START) {
            remote_attestation_start_in_ns = get_log_time_stamp(&line, runtime_type).unwrap();
            debug!("REMOTE_ATTESTATION_START {}", remote_attestation_start_in_ns);
        } else if line.contains(REMOTE_ATTESTATION_END) {
            remote_attestation_end_in_ns = get_log_time_stamp(&line, runtime_type).unwrap();

            debug!("REMOTE_ATTESTATION_END {}", remote_attestation_end_in_ns);
        } else if line.contains(GENERATE_ATTESTATION_REPORT_START) {
            generate_attestation_report_start_in_ns = get_log_time_stamp(&line, runtime_type).unwrap();
            debug!("GENERATE_ATTESTATION_REPORT_START {}", generate_attestation_report_start_in_ns);

        } else if line.contains(GENERATE_ATTESTATION_REPORT_END) {
            generate_attestation_report_end_in_ns = get_log_time_stamp(&line, runtime_type).unwrap();
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

            let mut libs = LIB_MEASURED_BEFORE_APP_LAUNCHED_KEEPER.lock().unwrap();
            if !libs.contains(&lib_name) {
                libs.insert(lib_name);
            }
            debug!("LIB_MEASURED_BEFORE_APP_LAUNCHED {:?}", libs);
        } else if line.contains(LIB_MEASURED_DURING_RUNTIME) {
            let words: Vec<&str> = line.split_whitespace().collect();

            let lib_path: Vec<&str> = words[10].split("/").collect();
            let lib_name: String = lib_path[lib_path.len() - 1].strip_suffix("\"").unwrap().to_string();

            let mut libs = LIB_MEASURED_DURING_RUNTIME_KEEPER.lock().unwrap();
            if !libs.contains(&lib_name) {
                libs.insert(lib_name);
            }
            // runtime_meausrement_in_bytes = words[11].parse::<u64>().unwrap();
            debug!("LIB_MEASURED_DURING_RUNTIME {:?}", libs);
        } else if line.contains(SECRET_INJECTION_START) {
            secret_injection_start_in_ns = get_log_time_stamp(&line, runtime_type).unwrap();
            debug!("SECRET_INJECTION_START {}", secret_injection_start_in_ns);
        }  else if line.contains(SECRET_INJECTION_END) {
            secret_injection_end_in_ns = get_log_time_stamp(&line, runtime_type).unwrap();
            debug!("SECRET_INJECTION_END {}", secret_injection_end_in_ns);
        } 
    }

    let secret_injection_duration = secret_injection_end_in_ns - secret_injection_start_in_ns ;
    let get_report_duration = generate_attestation_report_end_in_ns - generate_attestation_report_start_in_ns; 
    let remote_attestation_and_provision_duration = remote_attestation_end_in_ns - remote_attestation_start_in_ns - get_report_duration;
    let app_running_time = app_exit_time_in_ns - app_start_time_in_ns;
    // time used by  base line component to set up env for contianer  + software measurement overhead
    let pure_app_lanch_time = app_start_time_in_ns - remote_attestation_and_provision_duration - get_report_duration - sandbox_start_time_in_ns;
    let total_app_lanch_time = app_start_time_in_ns  - sandbox_start_time_in_ns;
    let sandbox_exit_duration = sandbox_exit_time_in_ns - app_exit_time_in_ns;

    error!("round {:?}", round);
    info!("pure_app_lanch_time {}, total_app_lanch_time {}, app_running_time {}, remote_attestation_and_provision_duration {}, get_report_duration {}, secret_injection_duration {} measurement_in_byte_before_app_launch {} runtime_meausrement_in_bytes {}, sandbox_exit_duration {}", 
                                pure_app_lanch_time, total_app_lanch_time, app_running_time, remote_attestation_and_provision_duration, get_report_duration, secret_injection_duration, 
                                                measurement_in_byte_before_app_launch, runtime_meausrement_in_bytes, sandbox_exit_duration);


    let statistic = PodStatistic {
        remote_attestation_and_provision_duration_in_ms: (remote_attestation_and_provision_duration as f64 / 1000000.0),
        get_report_duration_in_ms: (get_report_duration as f64) / 1000000.0,
        secret_injection_duration_in_ms: (secret_injection_duration as f64) / 1000000.0,
        pure_app_lanch_duration_in_ms: (pure_app_lanch_time as f64) / 1000000.0,
        total_app_lanch_time_in_ms: (total_app_lanch_time as f64) / 1000000.0,
        app_running_dution_in_ms: (app_running_time as f64) / 1000000.0,
        sandbox_exit_duration_in_ms: (sandbox_exit_duration as f64) / 1000000.0,
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
    setup(RuntimeType::Cquark, WorkloadType::Nginx).unwrap();


    // parse_quark_log();
    
    // let path = std::path::PathBuf::from("/home/yaoxin/test/confidentail-quark-benchmart/redis.yaml");
    // let res = test_app_lauch_time(2, "redis".to_string(), path, WorkloadType::Redis).await?;

    perf_https_attestation_and_provisioning_client("nginx".to_string(), RuntimeType::Cquark).await.unwrap();
    // let path = std::path::PathBuf::from("/home/yaoxin/test/confidentail-quark-benchmart/ngnix.yaml");
    // let res = test_app_lauch_time(100, "nginx".to_string(), path, WorkloadType::Nginx).await?;
    // assert!(res == 100);
    Ok(())
}