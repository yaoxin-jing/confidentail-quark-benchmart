use tokio::process::Command;
use std::time::{Duration, SystemTime};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{
        Api, PostParams, ResourceExt, DeleteParams},
    runtime::wait::{await_condition, conditions::{is_pod_running, is_deleted}},
    Client,
};
use anyhow::Context;
use std::time::Instant;
use core::result::Result::Ok;
use crate::delete_pod;
use crate::execute_cmd;
const TEST_FINISHED: &str = "3 test passed";

async fn wait_for_test_complete (pod_name: &str, wait_time: u64) ->  anyhow::Result<()> {

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
            if line.contains(TEST_FINISHED) {
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



async fn log_test_result (pod_name: &str) ->  anyhow::Result<()> {

    let cmd = format!("kubectl logs {}", pod_name);


    let output = Command::new("sh")
    .args([
        "-c",
        &cmd
    ])
    .output()
    .await.map_err(|e| anyhow::Error::msg(format!("run_nginx_benchmark failed to run nginx benchmark, get error {:?}", e)))?;



    let res = String::from_utf8_lossy(&output.stdout);

    error!("========================result=====================");
    info!("log: {}", res);


    Ok(())

}


pub async fn test_get_attestation_report_syscall (pod_name: String, file_path: std::path::PathBuf) ->  anyhow::Result<()>{

    let client = Client::try_default().await?;
    let yaml = std::fs::read_to_string(&file_path).with_context(|| format!("Failed to read {}", file_path.display()))?;

    info!("yaml {}", yaml);
    let p: Pod = serde_yaml::from_str(&yaml)?;
    let pods: Api<Pod> = Api::default_namespaced(client);


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
                error!("test_get_attestation_report_syscall create pod got error, {:?}, pods.create got err {:?}", res, e);
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
            error!("test_get_attestation_report_syscall tokio::time::timeout(std::time::Duration::from_secs(60), running) {:?}", e);
            return Err(anyhow::Error::msg("test_get_attestation_report_syscall tokio::time::timeout(std::time::Duration::from_secs(60), running")); 
        }
    }

    
    wait_for_test_complete(&pod_name, 120).await.unwrap();


    log_test_result(&pod_name).await.unwrap();



    match delete_pod(&pod_name).await {
        Err(e) => {
            pods.delete(&pod_name, &DeleteParams::default().grace_period(0))
            .await?
            .map_left(|pdel| {
                assert_eq!(pdel.name_any(), pod_name);
            });
            error!("test_get_attestation_report_syscall delete_pod(&pod_name).await failed {:?}", e);
        },
        core::result::Result::Ok(_) => {
        }
    }


    Ok(())

}
