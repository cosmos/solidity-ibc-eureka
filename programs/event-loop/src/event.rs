pub struct MonitoringData;
pub struct AttestorData;

pub enum Event {
    Monitoring(MonitoringData),
    Attestor(AttestorData),
}
