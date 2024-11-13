use herodotus_sharp_playground::models::{JobResponse, ProverVersion, SharpSdk};
use tracing::info;
use url::Url;
pub struct Prover {
    pub api_key: String,
    pub url: Url,
}
impl Prover {
    pub fn new(api_key: String, url: Url) -> Self {
        Prover { api_key, url }
    }
    pub async fn submit_proof_generation(&self, pie: Vec<u8>) -> String {
        let base_url = "https://atlantic.api.herodotus.cloud";
        let sdk = SharpSdk::new(self.api_key.clone(), base_url).unwrap();
        let is_alive = sdk.get_is_alive().await.unwrap();
        if !is_alive {
            return "".to_string();
        }
        let id = sdk
            .proof_generation(pie, "dynamic", ProverVersion::Starkware)
            .await
            .unwrap()
            .sharp_query_id;
        id
    }
    pub async fn submit_atlantic_query(_proof: String) -> String {
        // TODO: Implement this
        "".to_string()
    }
    pub async fn fetch_proof(&self, query_id: &str) -> String {
        let base_url = "https://atlantic.api.herodotus.cloud/sharp_queries";
        let proof_path = format!("query_{}/proof.json", query_id);
        let client = reqwest::Client::new();
        let url = format!("{}/{}", base_url, proof_path);
        let response = client.get(&url).send().await.unwrap();
        let response_text = response.text().await.unwrap();
        response_text
    }

    pub async fn check_proof_generation_status(&self, query_id: &str) -> Option<JobResponse> {
        let base_url = "https://atlantic.api.herodotus.cloud";
        let sdk = SharpSdk::new(self.api_key.clone(), base_url).unwrap();
        let is_alive = sdk.get_is_alive().await.unwrap();
        if !is_alive {
            return None;
        }
        info!("Checking status for query_id {}", query_id);
        let status = sdk.get_sharp_query_jobs(&query_id).await.unwrap();
        Some(status)
    }
}
