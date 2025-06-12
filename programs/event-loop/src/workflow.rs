use std::future::Future;

pub trait DummyAttestor: 'static + Send + Sync {
    fn get_l2_data(&self) -> impl Future<Output = Result<(), &'static str>> + Send;
}

pub trait DummyMonitorer: 'static + Send + Sync {
    fn get_monitoring_results(&self) -> impl Future<Output = Result<(), &'static str>> + Send;
}

pub struct Att;

impl DummyAttestor for Att {
    async fn get_l2_data(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

pub struct Mon;

impl DummyMonitorer for Mon {
    async fn get_monitoring_results(&self) -> Result<(), &'static str> {
        Ok(())
    }
}
