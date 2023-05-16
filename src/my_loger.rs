

use crate::{Config, SharedLogger};
use anyhow::Ok;
use log::{set_boxed_logger, set_max_level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::io::Write;
use std::sync::Mutex;
use std::sync::Arc;
use simplelog::WriteLogger;
use simplelog::ConfigBuilder;
use std::fs::File;


pub struct MyLoger<W: Write + Send + 'static> {
    logger: Arc<Mutex<WriteLogger<W>>>,
    config: Config,
}

impl<W: Write + Send + 'static> Clone for MyLoger<W> {

    fn clone(&self) -> MyLoger<W> {

        MyLoger {
            logger: self.logger.clone(),
            config: self.config.clone(),
        }

    }
}


impl<W: Write + Send + 'static> MyLoger<W> {

    /// allows to create a new logger, that can be independently used, no matter what is globally set.
    ///
    /// no macros are provided for this case and you probably
    /// dont want to use this function, but `init()`, if you dont want to build a `CombinedLogger`.
    ///
    /// Takes the desired `Level`, `Config` and `Write` struct as arguments. They cannot be changed later on.
    ///
    /// # Examples
    /// ```
    /// # extern crate simplelog;
    /// # use simplelog::*;
    /// # use std::fs::File;
    /// # fn main() {
    /// let file_logger = WriteLogger::new(LevelFilter::Info, Config::default(), File::create("my_rust_bin.log").unwrap());
    /// # }
    /// ```
    pub fn new(log_level: LevelFilter, config: Config, writable: W) -> MyLoger<W> {
        let c = config.clone();

        let w =  WriteLogger::new(LevelFilter::Info, config, writable);
        
        
    MyLoger {
            logger: Arc::new(Mutex::new(*w)),
            config: c,
        }
    }


    pub fn reset_file_path(&mut self, log_level: LevelFilter, config: Config, writable: W) -> anyhow::Result<()> {
        let c = config.clone();

        let w =  WriteLogger::new(log_level, config, writable);
        
        let mut mutable = self.logger.lock()
        .map_err(|e| anyhow::Error::msg(format!("reset_file_path failed  get error {:?}", e)))?;;

        self.config = c;

        *mutable = *w;
        Ok(())
    }
}




impl<W: Write + Send + 'static> Log for MyLoger<W> {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.logger.lock().unwrap().enabled(metadata)
    }

    fn log(&self, record: &Record<'_>) {
        self.logger.lock().unwrap().log(record)
    }

    fn flush(&self) {
        self.logger.lock().unwrap().flush()
    }
}



impl<W: Write + Send + 'static> SharedLogger for MyLoger<W> {
    fn level(&self) -> LevelFilter {
        self.logger.lock().unwrap().level()
    }

    fn config(&self) -> Option<&Config> {
        Some(&self.config)
    }

    fn as_log(self: Box<Self>) -> Box<dyn Log> {
        Box::new(*self)
    }
}


pub fn reset_log_file(my_logger: &mut MyLoger<File>, file_name: &str) -> anyhow::Result<()> {


    let time_format = simplelog::format_description!("[year]:[month]:[day]:[hour]:[minute]:[second].[subsecond]");
    let config = ConfigBuilder::new()
    .set_time_format_custom(time_format)
    .build();
    my_logger.reset_file_path(log::LevelFilter::Info, config, File::create(file_name).unwrap()).unwrap();


    Ok(())

}


