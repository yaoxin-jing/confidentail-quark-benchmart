
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use tracing::*;
use anyhow::{Context, bail};
use kube::{
    api::{
        Api, DynamicObject, GroupVersionKind, DeleteParams, PostParams, ResourceExt, WatchEvent, WatchParams,PatchParams
        , Patch},
    runtime::{watcher, WatchStreamExt},
    discovery::{ApiCapabilities, ApiResource, Discovery, Scope},
    Client,
};
use tokio::{io::AsyncWriteExt, time::sleep_until};



async fn test_app_lauch_time(loop_times: i32, pod_name: String, file_path: std::path::PathBuf) ->  anyhow::Result<i32>{

    let client = Client::try_default().await?;
    let yaml = std::fs::read_to_string(&file_path).with_context(|| format!("Failed to read {}", file_path.display()))?;
    let p: Pod = serde_yaml::from_str(&yaml)?;
    let pods: Api<Pod> = Api::default_namespaced(client);
    let fileds_selector = format!("metadata.name={}", pod_name);

    let mut i = 0;

    while i < loop_times {
        info!("loop {}, pod_name {:?}", i, pod_name);
        pods.create(&PostParams::default(), &p).await?;
        // Wait until the pod is running, otherwise we get 500 error.
        let wp = WatchParams::default().fields(&fileds_selector).timeout(10);
        let mut stream = pods.watch(&wp, "0").await?.boxed();
        while let Some(status) = stream.try_next().await? {
            match status {
                WatchEvent::Added(o) => {
                    info!("Added {}", o.name_any());
                }
                WatchEvent::Modified(o) => {
                    let s = o.status.as_ref().expect("status exists on pod");
                    if s.phase.clone().unwrap_or_default() == "Running" {
                        info!("Pod is running {}", o.name_any());
                        break;
                    }
                }
                _ => {}
            }
        }

        // tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        let delete_params = DeleteParams::default().grace_period(0);
        // Delete it
        pods.delete(&pod_name, &delete_params)
            .await?
            .map_right(|pdel| {
            });
        
        let wp = WatchParams::default().fields(&fileds_selector).timeout(10);
        let mut stream = pods.watch(&wp, "0").await?.boxed();
        while let Some(status) = stream.try_next().await? {
            match status {
                WatchEvent::Deleted(o) => {
                    let s = o.status.as_ref().expect("status exists on pod");
                    if s.phase.clone().unwrap_or_default() == "Running" {
                        info!("loop {} pod {} deleted", i, o.name_any());
                        break;
                    }
                }
                _ => {}
            }
        }
        i = i + 1;
    }

    Ok(i)    
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let path = std::path::PathBuf::from("/home/yaoxin/test/mongo.yaml");
    let res = test_app_lauch_time(10, "quark-pod-mongo".to_string(), path).await?;
    assert!(res == 10);

    println!("test finished {}", res);
    Ok(())
}