pub struct Relayer {
    /// The configuration for the relayer.
    pub config: Config,
    /// The modules for the relayer.
    pub modules: Vec<Box<dyn Module>>,
}
