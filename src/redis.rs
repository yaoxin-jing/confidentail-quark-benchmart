

use std::process::Output;
use std::process::Command;
use crate::get_statistic;
use crate::StatisticKeeper;

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
pub struct RedisStatistic {
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



pub fn collect_redis_statistics(redis_output : Output) -> anyhow::Result<RedisStatistic> {

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


pub fn calculate_redis_result(statistic_keeper: &std::sync::MutexGuard<StatisticKeeper>) {


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


pub async fn run_redis_benchmark(pod_ip: String) ->  anyhow::Result<Output>{

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

    info!("stdout: {}", res);
    // println!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    Ok(output)
}
