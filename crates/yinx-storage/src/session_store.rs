use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    pub request_method: Option<String>,
    pub request_url: Option<String>,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<String>,
    pub request_body_type: Option<String>,
    pub request_auth_type: Option<String>,
    pub auth_username: Option<String>,
    pub auth_password: Option<String>,
    pub auth_token: Option<String>,
    pub request_params: Vec<(String, String)>,
    pub selected_tab: Option<usize>,
}

impl SessionState {
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &json)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn load_from(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let state: SessionState = serde_json::from_str(&content)?;
        Ok(state)
    }

    pub fn session_path(cwd: &std::path::Path) -> std::path::PathBuf {
        let dir_hash = simple_hash(cwd.to_string_lossy().as_ref());
        let state_dir = yinx_core::paths::state_dir();
        state_dir.join("sessions").join(format!("{}.json", dir_hash))
    }
}

fn simple_hash(s: &str) -> String {
    let mut h = 0u64;
    for b in s.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u64);
    }
    format!("{:016x}", h)
}
