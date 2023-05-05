
#[macro_use]
extern crate lazy_static;

#[macro_use] 
extern crate log;

use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{
        Api, DynamicObject, GroupVersionKind, DeleteParams, PostParams, ResourceExt, WatchEvent, WatchParams,PatchParams
        , Patch},
    runtime::{watcher, WatchStreamExt},
    discovery::{ApiCapabilities, ApiResource, Discovery, Scope},
    runtime::wait::{await_condition, conditions::{is_pod_running, is_deleted}, self},
    Client,
};
use std::process::{Command, Child};
use tokio::{
    io::{AsyncRead, AsyncWrite},
};
use anyhow::Context;
use std::time::SystemTime;
use chrono::{offset::Utc, DateTime};
use std::sync::Mutex;

use simplelog::*;


#[derive(Debug)]
enum WorkloadType {
    Redis,
    Nginx
}


lazy_static! {
    static ref tmp_dir_name: Mutex<String> = Mutex::new(String::default());
}

async fn run_redis_benchmark(pod_ip: String) ->  anyhow::Result<()>{

    // Compile code.
    let cmd = format!("redis-benchmark -h {} -p 6379 -n 100", pod_ip);

    let mut output = Command::new("bash")
    .args([
        "-c",
        &cmd
    ])
    .output()
    .expect("failed to execute process");

    println!("status: {}", output.status);
    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    Ok(())
}



async fn run_ngnix_benchmark(pod_ip: String) ->  anyhow::Result<()>{

    // Compile code.
    let cmd = format!("redis-benchmark -h {} -p 6379 -n 100", pod_ip);

    let mut output = Command::new("bash")
    .args([
        "-c",
        &cmd
    ])
    .output()
    .expect("failed to execute process");

    println!("status: {}", output.status);
    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    Ok(())
}



// async fn run_wordload(loop_times: i32, workload_type: workload_type) ->  anyhow::Result<()>{

//     Ok(())
// }

async fn forward_connection(
    pods: &Api<Pod>,
    pod_name: &str,
    port: u16,
    mut client_conn: impl AsyncRead + AsyncWrite + Unpin,
) -> anyhow::Result<()> {
    let mut forwarder = pods.portforward(pod_name, &[port]).await?;
    let mut upstream_conn = forwarder
        .take_stream(port)
        .context("port not found in forwarder")?;
    tokio::io::copy_bidirectional(&mut client_conn, &mut upstream_conn).await?;
    drop(upstream_conn);
    forwarder.join().await?;
    info!("connection closed");
    Ok(())

}

async fn port_forwarding(pod_name: String, locol_port: u16, pod_port: u16, pods: &Api<Pod>) ->  anyhow::Result<(Child)>{
    let cmd = format!("kubectl port-forward {} {}:{}", pod_name, locol_port, locol_port);

    let child = Command::new("bash")
    .args([
        "-c",
        &cmd
    ])
    .spawn()
    .expect("failed to execute port_forwarding");

    // let output = child.stdout.as_ref().unwrap();

    // println!("port_forwarding status: {}", output.status);
    // println!("port_forwarding stdout: {}", String::from_utf8_lossy(&child.stdout.unwrap()));
    // println!("port_forwarding stderr: {}", String::from_utf8_lossy(&child.stderr));

    Ok(child)
    // let client = Client::try_default().await?;
    // let pods: Api<Pod> = Api::default_namespaced(client);
    // let addr = SocketAddr::from(([127, 0, 0, 1], locol_port));
    // info!(local_addr = %addr, pod_port, "forwarding traffic to the pod");
    // info!("try opening http://{0} in a browser, or `curl http://{0}`", addr);
    // info!("use Ctrl-C to stop the server and delete the pod");
    // let server = TcpListenerStream::new(TcpListener::bind(addr).await.unwrap())
    //     .take_until(tokio::signal::ctrl_c())
    //     .try_for_each(|client_conn| async {
    //         if let Ok(peer_addr) = client_conn.peer_addr() {
    //             info!(%peer_addr, "new connection");
    //         }
    //         let pods = pods.clone();
    //         let pod_name = pod_name.clone();
    //         tokio::spawn(async move {
    //             if let Err(e) = forward_connection(&pods, &pod_name, pod_port, client_conn).await {
    //                 error!(
    //                     error = e.as_ref() as &dyn std::error::Error,
    //                     "failed to forward connection"
    //                 );
    //             }
    //         });
    //         // keep the server running
    //         Ok(())
    //     });
    // if let Err(e) = server.await {
    //     error!(error = &e as &dyn std::error::Error, "server error");
    // }

    // Ok(())


}



async fn test_app_lauch_time(loop_times: i32, pod_name: String, file_path: std::path::PathBuf, local_port: u16) ->  anyhow::Result<i32>{

    let client = Client::try_default().await?;
    let yaml = std::fs::read_to_string(&file_path).with_context(|| format!("Failed to read {}", file_path.display()))?;
    let p: Pod = serde_yaml::from_str(&yaml)?;
    let pods: Api<Pod> = Api::default_namespaced(client);
    let fileds_selector = format!("metadata.name={}", pod_name);

    pods.create(&PostParams::default(), &p).await?;


    // Wait until the pod is running, otherwise we get 500 error.
    let running = await_condition(pods.clone(), &pod_name, is_pod_running());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(60), running).await?;
    let pod_uid = pods.get_metadata(&pod_name).await?.uid().expect("failed to get pod uid");
    let pod_cluster_ip = pods.get(&pod_name).await?.status.unwrap().pod_ip.unwrap();

    // let p1cpy = pods.get(&pod_name).await?.spec.unwrap();
    // let mut pf = pods.portforward(&pod_name, ports).await?;
    // let mut port = pf.take_stream(ports[0]).unwrap();
    info!("pod ip {}", pod_cluster_ip);

    run_redis_benchmark(pod_cluster_ip).await.unwrap();


    // tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    // child.kill().unwrap();
    let delete_params = DeleteParams::default().grace_period(0);
    // Delete it
    pods.delete(&pod_name, &DeleteParams::default())
        .await?
        .map_left(|pdel| {
            assert_eq!(pdel.name_any(), pod_name);
        });

    let deleted = await_condition(pods.clone(), &pod_name, is_deleted(&pod_uid));
    let _ = tokio::time::timeout(std::time::Duration::from_secs(60), deleted).await?;


    Ok(1)    
}

#[derive(Debug)]
enum RuntimeType {
    Baseline,
    Cquark,   
}


fn execute_cmd(cmd: &str) {

    let mut output = Command::new("bash")
    .args([
        "-c",
        cmd
    ])
    .output()
    .expect("failed to mkdir");

}

fn setup(runtime_type: RuntimeType, workload_type: WorkloadType) -> anyhow::Result<()> {

    let current_time = SystemTime::now();
    let datetime: DateTime<Utc> = current_time.into();

    let dir_name = format!("benchmark-{}-{:?}-{:?}", datetime.format("%d-%m-%Y-%T"), runtime_type, workload_type);
    let cmd = format!("mkdir -p {}", dir_name);
    {
        let mut dir = tmp_dir_name.lock().unwrap();

        *dir  = dir_name.clone();
    }

    let mut output = Command::new("bash")
    .args([
        "-c",
        &cmd
    ])
    .output()
    .expect("failed to mkdir");


    let mut output = Command::new("bash")
    .args([
        "-c",
        "sudo rm -f /var/log/quark/quark.log"
    ])
    .output()
    .expect("failed to mkdir");

    let mut output = Command::new("bash")
    .args([
        "-c",
        "sudo rm -f /var/log/quark/quark.log"
    ])
    .output()
    .expect("failed to mkdir");

    let mut current_path = std::env::current_dir().unwrap();
    current_path.push(&dir_name);

    println!("{:?}", current_path);
    
    assert!(std::env::set_current_dir(&current_path).is_ok());

    let time_format = simplelog::format_description!("[year]:[month]:[day]:[hour]:[minute]:[second].[subsecond]");
    use std::fs::File;
    let config = ConfigBuilder::new()
    .set_time_format_custom(time_format)
    .build();

    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Debug, config.clone(), TerminalMode::Mixed, ColorChoice::Auto),
            WriteLogger::new(LevelFilter::Debug, config, File::create("benchmark.log").unwrap()),
        ]
    ).unwrap();
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    // tracing_subscriber::fmt::init();

    // let path = std::path::PathBuf::from("/home/yaoxin/demo/benchmark/redis.yaml");
    // let ports: u16 = 6379;
    // let res = test_app_lauch_time(50, "redis".to_string(), path, ports).await?;
    // // assert!(res == 50);

    // println!("test finished {}", res);
    setup(RuntimeType::Baseline, WorkloadType::Nginx).unwrap();


    info!("set up 11111111111");
    Ok(())
}