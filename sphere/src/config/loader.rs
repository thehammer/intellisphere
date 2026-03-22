use figment::{
    providers::{Env, Format, Yaml},
    Figment,
};

use super::SphereConfig;

impl SphereConfig {
    /// Load configuration from YAML file(s) and environment variables.
    ///
    /// Priority (highest to lowest):
    /// 1. Environment variables prefixed with SPHERE_
    /// 2. config/dyson.config.dev.yaml (if exists)
    /// 3. config/dyson.config.yaml
    #[allow(clippy::result_large_err)]
    pub fn load() -> Result<Self, figment::Error> {
        Figment::new()
            .merge(Yaml::file("config/dyson.config.yaml"))
            .merge(Yaml::file("config/dyson.config.dev.yaml"))
            .merge(Env::prefixed("SPHERE_").split("__"))
            .extract()
    }
}
