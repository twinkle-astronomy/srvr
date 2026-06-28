use dioxus::prelude::*;

use crate::frontend::server_fns::{self, ServerInfo};
use crate::models::{AuthenticatedUser, Device, Template};

#[derive(Clone, Copy)]
pub struct AppStore {
    // Auth / setup state
    pub needs_setup: Signal<Option<bool>>,
    pub current_user: Signal<Option<AuthenticatedUser>>,
    pub current_user_loaded: Signal<bool>,

    // Data collections
    pub devices: Signal<Vec<Device>>,
    pub templates: Signal<Vec<Template>>,
    pub users: Signal<Vec<AuthenticatedUser>>,
    pub server_info: Signal<Option<ServerInfo>>,

    // Distinguish loading-spinner from genuinely-empty lists
    pub devices_loaded: Signal<bool>,
    pub templates_loaded: Signal<bool>,
    pub users_loaded: Signal<bool>,
}

impl AppStore {
    pub fn new() -> Self {
        Self {
            needs_setup: Signal::new(None),
            current_user: Signal::new(None),
            current_user_loaded: Signal::new(false),
            devices: Signal::new(vec![]),
            templates: Signal::new(vec![]),
            users: Signal::new(vec![]),
            server_info: Signal::new(None),
            devices_loaded: Signal::new(false),
            templates_loaded: Signal::new(false),
            users_loaded: Signal::new(false),
        }
    }

    // --- Auth / setup ---

    pub async fn fetch_needs_setup(mut self) {
        match server_fns::check_needs_setup().await {
            Ok(v) => self.needs_setup.set(Some(v)),
            Err(e) => {
                tracing::error!("fetch_needs_setup failed: {e}");
                self.needs_setup.set(Some(false));
            }
        }
    }

    pub async fn fetch_current_user(mut self) {
        match server_fns::check_auth().await {
            Ok(user) => self.current_user.set(user),
            Err(e) => {
                tracing::error!("fetch_current_user failed: {e}");
                self.current_user.set(None);
            }
        }
        self.current_user_loaded.set(true);
    }

    // --- Server info ---

    pub async fn fetch_server_info(mut self) {
        match server_fns::get_server_info().await {
            Ok(info) => self.server_info.set(Some(info)),
            Err(e) => tracing::error!("fetch_server_info failed: {e}"),
        }
    }

    // --- Devices ---

    pub async fn fetch_devices(mut self) {
        match server_fns::get_devices().await {
            Ok(list) => self.devices.set(list),
            Err(e) => tracing::error!("fetch_devices failed: {e}"),
        }
        self.devices_loaded.set(true);
    }

    #[cfg(feature = "web")]
    pub fn upsert_device(mut self, device: Device) {
        let mut list = self.devices.write();
        if !list.iter().any(|d| d.id == device.id) {
            list.push(device);
        }
    }

    pub async fn delete_device(mut self, id: i64) -> Result<(), ServerFnError> {
        server_fns::delete_device(id).await?;
        self.devices.write().retain(|d| d.id != id);
        Ok(())
    }

    pub async fn update_device_template(
        mut self,
        device_id: i64,
        template_id: i64,
    ) -> Result<(), ServerFnError> {
        server_fns::update_device_template(device_id, template_id).await?;
        if let Some(d) = self.devices.write().iter_mut().find(|d| d.id == device_id) {
            d.template_id = template_id;
        }
        Ok(())
    }

    pub async fn update_device_maximum_compatibility(
        mut self,
        device_id: i64,
        val: bool,
    ) -> Result<(), ServerFnError> {
        server_fns::update_device_maximum_compatibility(device_id, val).await?;
        if let Some(d) = self.devices.write().iter_mut().find(|d| d.id == device_id) {
            d.maximum_compatibility = val;
        }
        Ok(())
    }

    // --- Templates ---

    pub async fn fetch_templates(mut self) {
        match server_fns::get_templates().await {
            Ok(list) => self.templates.set(list),
            Err(e) => tracing::error!("fetch_templates failed: {e}"),
        }
        self.templates_loaded.set(true);
    }

    pub async fn create_template(
        mut self,
        name: String,
        content: String,
    ) -> Result<Template, ServerFnError> {
        let t = server_fns::create_template(name, content).await?;
        self.templates.write().push(t.clone());
        Ok(t)
    }

    pub async fn copy_template(mut self, id: i64) -> Result<Template, ServerFnError> {
        let t = server_fns::copy_template(id).await?;
        self.templates.write().push(t.clone());
        Ok(t)
    }

    pub async fn save_template(
        mut self,
        id: i64,
        name: String,
        content: String,
    ) -> Result<(), ServerFnError> {
        server_fns::save_template(id, name.clone(), content.clone()).await?;
        if let Some(t) = self.templates.write().iter_mut().find(|t| t.id == id) {
            t.name = name;
            t.content = content;
        }
        Ok(())
    }

    pub async fn delete_template(mut self, id: i64) -> Result<(), ServerFnError> {
        server_fns::delete_template(id).await?;
        self.templates.write().retain(|t| t.id != id);
        Ok(())
    }

    // --- Users ---

    pub async fn fetch_users(mut self) {
        match server_fns::get_all_users().await {
            Ok(list) => self.users.set(list),
            Err(e) => tracing::error!("fetch_users failed: {e}"),
        }
        self.users_loaded.set(true);
    }

    pub async fn delete_user(mut self, user_id: i64) -> Result<(), ServerFnError> {
        server_fns::delete_user(user_id).await?;
        self.users.write().retain(|u| u.id != user_id);
        Ok(())
    }
}
