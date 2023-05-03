
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use tracing::*;
use anyhow::{Context, Result};
use kube::{
    api::{
        Api, DeleteParams, PostParams, ResourceExt, WatchEvent, WatchParams},
    Client,
};
use tokio::process::Command;


async fn test_exec_cmd(loop_times: i32, pod_name: String, file_path: std::path::PathBuf) ->  anyhow::Result<()>{

        // Compile code.
        let mut child = Command::new("bash")
        .args([
            "-c",
            "echo 'Hello'; sleep 3s; echo 'World'"
        ])
        .output()
        .await.expect("failed to execute process");

    Ok(())


}



async fn is_pod_ready(pod_name: &str) -> bool {

    let mut child = Command::new("bash")
    .args([
        "-c",
        "kubectl get pod"
    ])
    .output()
    .await.expect("failed to get pod");

    let stdout = child.stdout;
    let s: std::borrow::Cow<str> = String::from_utf8_lossy(&stdout);
    
    let mut target_line = Vec::new();
    let mut found: bool = false;
    for line in s.lines() {
        let vec_str: Vec<&str> = line.split_whitespace().collect();
        if vec_str[0].eq(pod_name) {
            target_line = vec_str;
            found = true;
        }
    }

    if !found {
        return false;
    }

    println!("{:?}", target_line);
    let status = target_line[2];
    match status {
        "Running" => {
            info!("pod is running");
            return true;
        }
        _ => {
            info!("pedding {}", status);
            return false
        },
        
    }
}


async fn clean_up() -> Result<()> {
    let cmd = format!("sudo pkill -9 containerd-shim");

    let mut child = Command::new("bash")
    .args([
        "-c",
        &cmd
    ])
    .output()
    .await.expect("failed to delete pod");

    info!("{:?}", child);

    Ok(())
}


async fn test_app_lauch_time(loop_times: i32, pod_name: String, file_path: std::path::PathBuf) ->  anyhow::Result<i32>{

    let client = Client::try_default().await?;
    let yaml = std::fs::read_to_string(&file_path).with_context(|| format!("Failed to read {}", file_path.display()))?;
    let p: Pod = serde_yaml::from_str(&yaml)?;
    let pods: Api<Pod> = Api::default_namespaced(client);
    let fileds_selector = format!("metadata.name={}", pod_name);
    let mut i = 0;

    let delete_pod = format!("kubectl delete pod {} --grace-period=0 --force", pod_name);

    let mut child = Command::new("bash")
    .args([
        "-c",
        &delete_pod
    ])
    .output()
    .await.expect("failed to delete pod");

    while i < loop_times {
        info!("loop {}, pod_name {:?}", i, pod_name);
        // pods.create(&PostParams::default(), &p).await?;

        let mut child = Command::new("bash")
        .args([
            "-c",
            "kubectl apply -f /home/yaoxin/demo/benchmark/mongodb.yaml"
        ])
        .output()
        .await.expect("failed to execute process");


        
        while !(is_pod_ready(&pod_name).await) {}
        

        let delete_pod = format!("kubectl delete pod {} --grace-period=0 --force", pod_name);

        let mut child = Command::new("bash")
        .args([
            "-c",
            &delete_pod
        ])
        .output()
        .await.expect("failed to delete pod");

        info!("{:?}", child);

        

        i = i + 1;
    }
    // clean_up().await.unwrap();

    Ok(i)    
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mongo_path = std::path::PathBuf::from("/home/yaoxin/demo/benchmark/mongodb.yaml");
    let nginx_path = std::path::PathBuf::from("/home/yaoxin/demo/benchmark/nginx.yaml");
    let res = test_app_lauch_time(150, "quark-pod-mongo1".to_string(), mongo_path).await?;
    assert!(res == 150);

    println!("test finished {}", res);
    Ok(())
}