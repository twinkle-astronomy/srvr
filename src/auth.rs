use axum::{
    extract::Form,
    response::{IntoResponse, Redirect},
    routing::post,
    Router,
};
use axum_login::AuthnBackend;
use serde::Deserialize;

use crate::models::User;

impl axum_login::AuthUser for User {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
    }
}

#[derive(Clone)]
pub struct Backend;

#[derive(Clone, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = sqlx::Error;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        use argon2::{Argon2, PasswordHash, PasswordVerifier};

        let user = crate::db::get_user_by_username(&creds.username).await?;
        let Some(user) = user else {
            return Ok(None);
        };

        let parsed_hash = match PasswordHash::new(&user.password_hash) {
            Ok(h) => h,
            Err(_) => return Ok(None),
        };

        if Argon2::default()
            .verify_password(creds.password.as_bytes(), &parsed_hash)
            .is_ok()
        {
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }

    async fn get_user(&self, id: &axum_login::UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        crate::db::get_user_by_id(*id).await
    }
}

pub type AuthSession = axum_login::AuthSession<Backend>;

fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
    use argon2::password_hash::rand_core::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default().hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

async fn login(mut auth: AuthSession, Form(creds): Form<Credentials>) -> impl IntoResponse {
    match auth.authenticate(creds).await {
        Ok(Some(user)) => {
            if auth.login(&user).await.is_err() {
                return Redirect::to("/login?error=session").into_response();
            }
            Redirect::to("/").into_response()
        }
        Ok(None) => Redirect::to("/login?error=invalid").into_response(),
        Err(_) => Redirect::to("/login?error=server").into_response(),
    }
}

async fn logout(mut auth: AuthSession) -> impl IntoResponse {
    auth.logout().await.ok();
    Redirect::to("/login")
}

async fn setup(mut auth: AuthSession, Form(creds): Form<Credentials>) -> impl IntoResponse {
    let count = match crate::db::user_count().await {
        Ok(c) => c,
        Err(_) => return Redirect::to("/setup?error=server").into_response(),
    };

    if count > 0 {
        return Redirect::to("/login").into_response();
    }

    if creds.username.is_empty() || creds.password.is_empty() {
        return Redirect::to("/setup?error=empty").into_response();
    }

    let password_hash = match hash_password(&creds.password) {
        Ok(h) => h,
        Err(_) => return Redirect::to("/setup?error=server").into_response(),
    };

    let user = match crate::db::create_user(&creds.username, &password_hash).await {
        Ok(u) => u,
        Err(_) => return Redirect::to("/setup?error=server").into_response(),
    };

    if auth.login(&user).await.is_err() {
        return Redirect::to("/login").into_response();
    }

    Redirect::to("/").into_response()
}

async fn create_user(auth: AuthSession, Form(creds): Form<Credentials>) -> impl IntoResponse {
    if auth.user.is_none() {
        return Redirect::to("/login").into_response();
    }

    if creds.username.is_empty() || creds.password.is_empty() {
        return Redirect::to("/users?error=empty").into_response();
    }

    let password_hash = match hash_password(&creds.password) {
        Ok(h) => h,
        Err(_) => return Redirect::to("/users?error=server").into_response(),
    };

    match crate::db::create_user(&creds.username, &password_hash).await {
        Ok(_) => Redirect::to("/users").into_response(),
        Err(_) => Redirect::to("/users?error=exists").into_response(),
    }
}

#[derive(Deserialize)]
struct ChangePassword {
    current_password: String,
    new_password: String,
}

async fn change_password(
    auth: AuthSession,
    Form(form): Form<ChangePassword>,
) -> impl IntoResponse {
    let Some(user) = auth.user else {
        return Redirect::to("/login").into_response();
    };

    if form.new_password.is_empty() {
        return Redirect::to("/users?error=empty").into_response();
    }

    // Verify current password
    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    let parsed_hash = match PasswordHash::new(&user.password_hash) {
        Ok(h) => h,
        Err(_) => return Redirect::to("/users?error=server").into_response(),
    };
    if Argon2::default()
        .verify_password(form.current_password.as_bytes(), &parsed_hash)
        .is_err()
    {
        return Redirect::to("/users?error=wrong_password").into_response();
    }

    // Hash and save new password
    let new_hash = match hash_password(&form.new_password) {
        Ok(h) => h,
        Err(_) => return Redirect::to("/users?error=server").into_response(),
    };

    match crate::db::update_user_password(user.id, &new_hash).await {
        Ok(_) => Redirect::to("/users?success=password_changed").into_response(),
        Err(_) => Redirect::to("/users?error=server").into_response(),
    }
}

pub async fn server_fn_auth_middleware(
    auth: AuthSession,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{Method, StatusCode};

    // GET requests (SSR pages, assets) pass through — frontend handles redirects
    if request.method() != Method::POST {
        return next.run(request).await;
    }

    // Allowlist: public server functions that work without auth
    let path = request.uri().path();
    if path.contains("check_auth") || path.contains("check_needs_setup") {
        return next.run(request).await;
    }

    // All other POST requests (server functions) require auth
    if auth.user.is_some() {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

pub fn router() -> Router {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/setup", post(setup))
        .route("/auth/create-user", post(create_user))
        .route("/auth/change-password", post(change_password))
}
