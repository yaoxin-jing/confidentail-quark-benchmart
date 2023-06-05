use anyhow::Ok;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{
        Api, PostParams, ResourceExt, DeleteParams},
    runtime::wait::{await_condition, conditions::is_pod_running},
    Client,
};
use tokio::time::error;
use tracing_subscriber::fmt::format;
use std::{time::Instant, collections::HashMap};
use crate::{WorkloadType, RuntimeType};
use crate::GLOBAL_STATISTICS_KEEPER;
use crate::execute_cmd;
use k8s_openapi::api::core::v1::EnvVar;
use crate::parse_quark_log;
use crate::StatisticKeeper;
use crate::get_statistic;
use crate::SANDBOX_START;
use crate::delete_pod;
use crate::get_log_time_stamp;
use std::io::BufReader;
use std::io::BufRead;
use crate::MyLoger;
use std::fs::File;
use rev_lines::RevLines;
use crate::my_loger::reset_log_file;

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
            \"image\": \"yaoxinjing/micro-bench-pasue:latest\",
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
                \"value\": \"quark_mongo/mongo_resource/policy\"
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

async fn create_pod(pod_name: &str) -> anyhow::Result<()> {

    let client = Client::try_default().await?;
    // Manage pods
    let pods: Api<Pod> = Api::default_namespaced(client);

    let  p: Pod = serde_json::from_str(POD_TEMPLATE)?;

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
        core::result::Result::Ok(_) => {},
        Err(e) => {
            pods.delete(&pod_name, &DeleteParams::default().grace_period(0))
            .await?
            .map_left(|pdel| {
                assert_eq!(pdel.name_any(), pod_name);
            });
            error!("tokio::time::timeout(std::time::Duration::from_secs(60), running) {:?}", e);
            return Err(anyhow::Error::msg("tokio::time::timeout(std::time::Duration::from_secs(60), running")); 
        }
    }

    Ok(())

}


// /home/yaoxin/test/Trusted_Client/target/debug/secure-client


pub async fn perf_kubectl_cmd (cmds_list: Vec<String>, loop_time: i32, my_logger: &mut MyLoger<File>, runtime_type: &RuntimeType) ->  anyhow::Result<()> {

    info!("perf_kubectl_cmd-cmd list-{:?}-loop-time-{:?}", cmds_list, loop_time);

    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::default_namespaced(client);


    let pod_name = "micro".to_string();

    let log_file_name = format!("perf_kubectl_cmd-cmd--loop-time-{:?}", loop_time);


    info!("cmd list {:?}", cmds_list);
    reset_log_file(my_logger, &log_file_name).unwrap();


    let mut statistics :HashMap<String, kubectl_statistic> = HashMap::new();

    create_pod(&pod_name).await.unwrap();

    for cmd in cmds_list {

        let kubectl_cmd = format!("kubectl exec {} {}", pod_name, cmd);

        let mut cmd_execute_duration = Vec::new();
        let mut exex_total_time_in_qkernel = Vec::new();
        let mut exec_check_time = Vec::new();
        let mut dexecution_cmd_time_in_qkernel = Vec::new();
        let mut i = 0;

        loop {
            if i >= loop_time {
                break;
            }  
    
            let start = Instant::now();
            let output = execute_cmd(&kubectl_cmd);
            let deleted = Instant::now();

            let duration = deleted - start;
            cmd_execute_duration.push(duration.as_millis() as f64);

            let res = String::from_utf8_lossy(&output.stdout);
            info!("cmd {:?} result: {}", cmd, res);
            i = i + 1;

            let data = get_data(false, runtime_type).unwrap();


            let check_time : u128= if runtime_type == &RuntimeType::Baseline {
                0
            } else {
                data.exec_check_end_time - data.exec_check_start_time
            };

            let cmd_execution = data.exec_exec_end_time - data.exec_exec_start_time;

            let total_time_in_qkernel =  data.exec_exec_end_time - data.exec_check_start_time;

            exex_total_time_in_qkernel.push(total_time_in_qkernel as f64);
            exec_check_time.push(check_time as f64);
            dexecution_cmd_time_in_qkernel.push(cmd_execution as f64);

        }

        
        let kubectl_statistic = get_statistic(&cmd_execute_duration).unwrap();

        let exex_total_time_in_qkernel_statistic = get_statistic(&exex_total_time_in_qkernel).unwrap();

        let exec_check_time_statistic = get_statistic(&exec_check_time).unwrap();

        let execution_cmd_time_in_qkernel_statistic = get_statistic(&dexecution_cmd_time_in_qkernel).unwrap();


        info!("kubectl cml cmd {:?}, kubectl_statistic {:?} exex_total_time_in_qkernel_statistic {:?}, exec_check_time_statistic {:?}, execution_cmd_time_in_qkernel_statistic {:?}", cmd, kubectl_statistic, exex_total_time_in_qkernel_statistic, exec_check_time_statistic, execution_cmd_time_in_qkernel_statistic);

        let kubectl_statistic = kubectl_statistic {
            cmd_execute_duration: cmd_execute_duration,
            exex_total_time_in_qkernel: exex_total_time_in_qkernel,
            exec_check_time: exec_check_time,
            dexecution_cmd_time_in_qkernel, 
        };

        statistics.insert(cmd, kubectl_statistic);

    }

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

    error!("================== Result ==============================");
    
    for (cmd, statistic) in statistics {



        let kubectl_statistic = get_statistic(&statistic.cmd_execute_duration).unwrap();

        let exex_total_time_in_qkernel_statistic = get_statistic(&statistic.exex_total_time_in_qkernel).unwrap();

        let exec_check_time_statistic = get_statistic(&statistic.exec_check_time).unwrap();

        let execution_cmd_time_in_qkernel_statistic = get_statistic(&statistic.dexecution_cmd_time_in_qkernel).unwrap();


        info!("kubectl cml cmd {:?}, kubectl_statistic {:?} exex_total_time_in_qkernel_statistic {:?}, exec_check_time_statistic {:?}, execution_cmd_time_in_qkernel_statistic {:?}", cmd, kubectl_statistic, exex_total_time_in_qkernel_statistic, exec_check_time_statistic, execution_cmd_time_in_qkernel_statistic);
    }

    Ok(())
}


const exec_req_authentication_start: &str = "exec_req_authentication start";
const exec_req_authentication_end: &str = "exec_req_authentication end";
const exec_start_execution: &str = "ExecProcess start";
const EXEC_UNPRIVELEGED_FINISH: &str = "exec req is ended";
const EXEC_PRIVELEGED_FINISH: &str = "exec req is ended";


pub struct Data {
    pub exec_check_start_time:u128,
    pub exec_check_end_time:u128,
    pub exec_exec_start_time:u128,
    pub exec_exec_end_time:u128,

}



pub fn get_data (is_priveleged: bool, runtime_type: &RuntimeType) ->  anyhow::Result<Data> {



    // let mut exec_check_start_time:u128 = 0;
    // let mut exec_check_end_time:u128 = 0;
    // let mut exec_exec_start_time:u128 = 0;
    // let mut exec_exec_end_time:u128 = 0;


    // let mut get_a = false;
    // let mut get_b = false;
    // let mut get_c = false;
    // let mut get_d = false;

    let mut exec_check_start_time:u128 = 0;
    let mut exec_check_end_time:u128 = 0;
    let mut exec_exec_start_time:u128 = 0;
    let mut exec_exec_end_time:u128 = 0;

    loop {

        let log_file = File::open("/var/log/quark/quark.log").unwrap();
        let rev_lines = RevLines::new(BufReader::new(log_file)).unwrap();


    
        let mut get_a = false;
        let mut get_b = false;
        let mut get_c = false;
        let mut get_d = false;

        for line in rev_lines {

            if line.contains(exec_req_authentication_start) {
                exec_check_start_time = get_log_time_stamp(&line, runtime_type).unwrap();
                get_a = true;
                break;    
            }
    
            if line.contains(exec_req_authentication_end) {
                exec_check_end_time = get_log_time_stamp(&line, runtime_type).unwrap() ; 
            }
    
            if line.contains(exec_start_execution) {
                exec_exec_start_time = get_log_time_stamp(&line, runtime_type).unwrap() ;
            }
    
            if is_priveleged {
                if line.contains(EXEC_PRIVELEGED_FINISH) {
                    exec_exec_end_time = get_log_time_stamp(&line, runtime_type).unwrap();
                    get_d = true;     
                }
    
            } else {
                if line.contains(EXEC_UNPRIVELEGED_FINISH) {
                    exec_exec_end_time = get_log_time_stamp(&line, runtime_type).unwrap();
                    get_d = true;        
                }
        
            }
        }

        if  get_a == true && get_d == true {
            break;
        }

        info!("{}, {}, {}, {}", get_a, get_b, get_c, get_d);
    }

    // assert!(get_b == true && get_a == true && get_c == true && get_d == true);
    let data = Data {
        exec_check_start_time, 
        exec_check_end_time,
        exec_exec_start_time,
        exec_exec_end_time
    };
    Ok(data)
}

struct kubectl_statistic {
    cmd_execute_duration: Vec<f64>,
    exex_total_time_in_qkernel: Vec<f64>,
    exec_check_time : Vec<f64>,
    dexecution_cmd_time_in_qkernel: Vec<f64>,
}






pub async fn perf_securecli_cmd (cmds_list: Vec<String>, loop_time: i32, my_logger: &mut MyLoger<File>, runtime_type: &RuntimeType) ->  anyhow::Result<()> {

    info!("perf_kubectl_cmd-cmd list-{:?}-loop-time-{:?}", cmds_list, loop_time);

    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::default_namespaced(client);


    let pod_name = "micro".to_string();

    let log_file_name = format!("perf_securecli_cmd-cmd--loop-time-{:?}", loop_time);


    info!("cmd list {:?}", cmds_list);
    reset_log_file(my_logger, &log_file_name).unwrap();


    let mut statistics :HashMap<String, kubectl_statistic> = HashMap::new();

    execute_cmd("sudo rm -f /var/log/quark/quark.log");
    execute_cmd("sudo rm -f /home/yaoxin/test/Trusted_Client/target/debug/session.json");
    create_pod(&pod_name).await.unwrap();



    for cmd in cmds_list {

        let securecli_cmd = format!("/home/yaoxin/test/Trusted_Client/target/debug/secure-client  issue-cmd  {} {:?}", pod_name, cmd);

        info!("securecli_cmd {}", securecli_cmd);

        let mut cmd_execute_duration = Vec::new();
        let mut exex_total_time_in_qkernel = Vec::new();
        let mut exec_check_time = Vec::new();
        let mut dexecution_cmd_time_in_qkernel = Vec::new();
        let mut i = 0;

        loop {
            if i >= loop_time {
                break;
            }  
    
            let start = Instant::now();
            let output = execute_cmd(&securecli_cmd);
            let deleted = Instant::now();

            let duration = deleted - start;
            cmd_execute_duration.push(duration.as_millis() as f64);

            let res = String::from_utf8_lossy(&output.stdout);

            let data = get_data(true, runtime_type).unwrap();
            
            let check_time = data.exec_check_end_time - data.exec_check_start_time;
            let cmd_execution = data.exec_exec_end_time - data.exec_exec_start_time;

            let total_time_in_qkernel =  data.exec_exec_end_time - data.exec_check_start_time;

            exex_total_time_in_qkernel.push(total_time_in_qkernel as f64);
            exec_check_time.push(check_time as f64);
            dexecution_cmd_time_in_qkernel.push(cmd_execution as f64);


            info!("cmd {:?} result: {}", cmd, res);
            i = i + 1;
        }

        let kubectl_statistic = get_statistic(&cmd_execute_duration).unwrap();

        let exex_total_time_in_qkernel_statistic = get_statistic(&exex_total_time_in_qkernel).unwrap();

        let exec_check_time_statistic = get_statistic(&exec_check_time).unwrap();

        let execution_cmd_time_in_qkernel_statistic = get_statistic(&dexecution_cmd_time_in_qkernel).unwrap();


        info!("secure cml cmd {:?}, kubectl_statistic {:?} exex_total_time_in_qkernel_statistic {:?}, exec_check_time_statistic {:?}, execution_cmd_time_in_qkernel_statistic {:?}", cmd, kubectl_statistic, exex_total_time_in_qkernel_statistic, exec_check_time_statistic, execution_cmd_time_in_qkernel_statistic);

        let kubectl_statistic = kubectl_statistic {
            cmd_execute_duration: cmd_execute_duration,
            exex_total_time_in_qkernel: exex_total_time_in_qkernel,
            exec_check_time: exec_check_time,
            dexecution_cmd_time_in_qkernel, 
        };

        statistics.insert(cmd, kubectl_statistic);


    }

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

    error!("================== Result ==============================");
    
    for (cmd, statistic) in statistics {



        let kubectl_statistic = get_statistic(&statistic.cmd_execute_duration).unwrap();

        let exex_total_time_in_qkernel_statistic = get_statistic(&statistic.exex_total_time_in_qkernel).unwrap();

        let exec_check_time_statistic = get_statistic(&statistic.exec_check_time).unwrap();

        let execution_cmd_time_in_qkernel_statistic = get_statistic(&statistic.dexecution_cmd_time_in_qkernel).unwrap();


        info!("secure cml cmd {:?}, securectl_statistic {:?} exex_total_time_in_qkernel_statistic {:?}, exec_check_time_statistic {:?}, execution_cmd_time_in_qkernel_statistic {:?}", cmd, kubectl_statistic, exex_total_time_in_qkernel_statistic, exec_check_time_statistic, execution_cmd_time_in_qkernel_statistic);
    }

    Ok(())
}