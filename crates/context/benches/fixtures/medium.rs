//! A medium-sized Rust file for benchmarking.

use std::collections::HashMap;

/// Configuration for the application.
#[derive(Debug, Clone)]
pub struct Config {
    pub name: String,
    pub port: u16,
    pub debug: bool,
    pub features: Vec<String>,
}

impl Config {
    /// Load config from environment variables.
    pub fn from_env() -> Result<Self, String> {
        let name = std::env::var("APP_NAME").unwrap_or_else(|_| "default".to_string());
        let port = std::env::var("APP_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .map_err(|e| format!("invalid port: {}", e))?;
        let debug = std::env::var("APP_DEBUG").unwrap_or_else(|_| "false".to_string()) == "true";
        let features = std::env::var("APP_FEATURES")
            .unwrap_or_else(|_| "".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(Config {
            name,
            port,
            debug,
            features,
        })
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("name cannot be empty".to_string());
        }
        if self.port == 0 {
            return Err("port cannot be zero".to_string());
        }
        Ok(())
    }
}

/// A registry of services.
pub struct Registry {
    services: HashMap<String, Box<dyn Service>>,
}

/// Trait for services.
pub trait Service: Send + Sync {
    fn name(&self) -> &str;
    fn start(&mut self) -> Result<(), String>;
    fn stop(&mut self) -> Result<(), String>;
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            services: HashMap::new(),
        }
    }

    pub fn register(&mut self, service: Box<dyn Service>) {
        let name = service.name().to_string();
        self.services.insert(name, service);
    }

    pub fn start_all(&mut self) -> Result<(), String> {
        for (name, service) in self.services.iter_mut() {
            service.start().map_err(|e| format!("{}: {}", name, e))?;
        }
        Ok(())
    }

    pub fn stop_all(&mut self) -> Result<(), String> {
        for (name, service) in self.services.iter_mut() {
            service.stop().map_err(|e| format!("{}: {}", name, e))?;
        }
        Ok(())
    }
}

/// A simple HTTP service.
pub struct HttpService {
    name: String,
    config: Config,
}

impl HttpService {
    pub fn new(config: Config) -> Self {
        let name = config.name.clone();
        HttpService { name, config }
    }
}

impl Service for HttpService {
    fn name(&self) -> &str {
        &self.name
    }

    fn start(&mut self) -> Result<(), String> {
        println!("Starting HTTP service on port {}", self.config.port);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        println!("Stopping HTTP service {}", self.name);
        Ok(())
    }
}
