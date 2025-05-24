use log::{info, error};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::SerializableError;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct HookConfig {
    /// success hook
    pub(crate) success: Option<String>,
    /// failure hook
    pub(crate) failure: Option<String>,
    /// partial hook
    pub(crate) partial: Option<String>,
}

impl HookConfig {
    pub fn success(&self) {
        if let Some(success_hook) = &self.success {
            let cli = Client::new();
            let res = cli
                .get(success_hook)
                .send()
                .expect("Failed to send success hook request");
                
            if res.status().is_success() {
                info!("fail hook executed successfully");
            } else {
                error!("fail hook failed with status: {}", res.status());
            }
        }
    }

    pub fn partial(&self, failed: Vec<String>) {
        if let Some(partial_hook) = &self.partial {
            let cli = Client::new();
            let res = cli
                .post(partial_hook)
                .header("Content-Type", "application/json")
                .json(&failed)
                .send()
                .expect("Failed to send partial hook request");
                
            if res.status().is_success() {
                info!("partial hook executed successfully");
            } else {
                error!("partial hook failed with status: {}", res.status());
            }
        }
    }

    pub fn failure(&self, e: SerializableError) {
        if let Some(failure_hook) = &self.failure {
            let cli = Client::new();
            let res = cli
                .post(failure_hook)
                .header("Content-Type", "application/json")
                .json(&e)
                .send()
                .expect("Failed to send success hook request");
                
            if res.status().is_success() {
                info!("success hook executed successfully");
            } else {
                error!("success hook failed with status: {}", res.status());
            }
        }
    }
}
