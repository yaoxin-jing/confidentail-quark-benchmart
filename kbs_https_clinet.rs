use k8s_openapi::api::core::v1::Pod;
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


fn calcaulate_https_client_statistic_result(statistic_keeper: &std::sync::MutexGuard<StatisticKeeper>) -> anyhow::Result<()> {

    let mut remote_attestation_and_provision_duration_in_ms: Vec<f64> = Vec::new();
    let mut get_report_duration_in_ms: Vec<f64> = Vec::new();
    let mut secret_injection_duration_in_ms: Vec<f64> = Vec::new();

    let mut pure_app_lanch_duration_in_ms: Vec<f64> = Vec::new();
    let mut total_app_lanch_time_in_ms:Vec<f64> = Vec::new();
    let mut app_running_dution_in_ms: Vec<f64> = Vec::new();
    let mut sandbox_exit_duration_in_ms: Vec<f64> = Vec::new();

    let mut total_period_in_s = Vec::new();

    for item in &statistic_keeper.pods_statistic {
        remote_attestation_and_provision_duration_in_ms.push(item.remote_attestation_and_provision_duration_in_ms);
        get_report_duration_in_ms.push(item.get_report_duration_in_ms);
        secret_injection_duration_in_ms.push(item.secret_injection_duration_in_ms);

        pure_app_lanch_duration_in_ms.push(item.pure_app_lanch_duration_in_ms);
        total_app_lanch_time_in_ms.push(item.total_app_lanch_time_in_ms);
        app_running_dution_in_ms.push(item.app_running_dution_in_ms);
        sandbox_exit_duration_in_ms.push(item.sandbox_exit_duration_in_ms);

        total_period_in_s.push(item.total_period_in_s);
    }

    let remote_attestation_and_provision_statistic_in_ms = get_statistic(&remote_attestation_and_provision_duration_in_ms).unwrap();
    let get_report_duration_in_ms_statistic = get_statistic(&get_report_duration_in_ms).unwrap();
    let secret_injection_duration_in_ms_statistic = get_statistic(&secret_injection_duration_in_ms).unwrap();

    let pure_app_lanch_duration_in_ms_statistic = get_statistic(&pure_app_lanch_duration_in_ms).unwrap();
    let total_app_lanch_time_in_ms_statistic = get_statistic(&total_app_lanch_time_in_ms).unwrap();

    let app_running_dution_in_ms_statistic = get_statistic(&app_running_dution_in_ms).unwrap();
    let sandbox_exit_duration_in_ms_statistic = get_statistic(&sandbox_exit_duration_in_ms).unwrap();

    let total_period_in_s_statistic = get_statistic(&total_period_in_s).unwrap();


    info!("remote_attestation_and_provision_statistic_in_ms: {:?}", remote_attestation_and_provision_statistic_in_ms);
    info!("get_report_duration_in_ms_statistic:  {:?}", get_report_duration_in_ms_statistic);
    info!("secret_injection_duration_in_ms_statistic: {:?}", secret_injection_duration_in_ms_statistic);


    info!("remote_attestation_and_provision_duration_in_ms: {:?}", remote_attestation_and_provision_duration_in_ms);
    info!("get_report_duration_in_ms: {:?}", get_report_duration_in_ms);
    info!("secret_injection_duration_in_ms: {:?}", secret_injection_duration_in_ms);

    info!("pure_app_lanch_duration_in_ms_statistic: {:?}", pure_app_lanch_duration_in_ms_statistic);
    info!("total_app_lanch_time_in_ms_statistic: {:?}", total_app_lanch_time_in_ms_statistic);
    info!("app_running_dution_in_ms_statistic: {:?}", app_running_dution_in_ms_statistic);
    info!("sandbox_exit_duration_in_ms_statistic: {:?}", sandbox_exit_duration_in_ms_statistic);

    // info!("measurement_before_app_launch_in_mb_statistic: {:?}", measurement_before_app_launch_in_mb_statistic);
    // info!("runtime_meausrement_in_mb_statistic: {:?}", runtime_meausrement_in_mb_statistic);
    info!("total_period_in_s_statistic: {:?}", total_period_in_s_statistic);

    Ok(())
}

    use crate::MyLoger;
    use std::fs::File;
    use crate::my_loger::reset_log_file;

pub async fn perf_https_attestation_and_provisioning_client (file_secret_num : i32, loop_time: i32, my_logger: &mut MyLoger<File>) ->  anyhow::Result<()> {

    let mut i = 1;

    let pod_name = "micro".to_string();
    
    let client = Client::try_default().await?;
    // Manage pods
    let pods: Api<Pod> = Api::default_namespaced(client);
    let mut p: Pod = serde_json::from_str(POD_TEMPLATE)?;

    let mut resource_urls = "files/0".to_string();
    let mut statistic_keeper = GLOBAL_STATISTICS_KEEPER.lock().unwrap();


    loop {
        if i >= file_secret_num {
            break;
        }
        let resource_url = format!(",files/{}", i);
        resource_urls.push_str(&resource_url);
        i = i + 1;


    }

    let mut j = 0;
    let env = EnvVar {
        name: "FILE_BASED_SECRETS_PATH".to_string(),
        value: Some(resource_urls.clone()),
        value_from: None,
    };


    let log_name = format!("file_secret_num-{:?}-loop_time-{:?}", file_secret_num, loop_time);
    reset_log_file(my_logger, &log_name).unwrap();

    info!("file_secret_num-{:?}-loop_time-{:?}-secret_urls {:?}", file_secret_num, loop_time, resource_urls);

    loop {
        if j >= loop_time {
            break;
        }  

        error!("============ round {:?} =============", j);
        execute_cmd("sudo rm -f /var/log/quark/quark.log");


        if file_secret_num != 0 {
            let _ = &mut p.spec.as_mut().unwrap().containers[0].env.as_mut().unwrap().push(env.clone());
        }

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

        let quark_statistic = match parse_quark_log(j, &RuntimeType::Cquark) {
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

        info!("resource_urls {:?}", resource_urls);
        
        j = j + 1;

    }

    error!("================== Result ==============================");
    crate::calcaulate_statistic_result(&statistic_keeper, WorkloadType::Micro(Vec::new())).unwrap();
    Ok(())
}