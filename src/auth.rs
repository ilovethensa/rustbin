use actix_identity::Identity;
use actix_web::{get, post, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use bcrypt::{hash, verify, DEFAULT_COST};
use serde::Deserialize;
use sqlx::FromRow;
use tera::Tera;

use crate::utils::is_valid_title;

#[derive(Debug, Clone, FromRow)]
struct User {
    username: String,
    password: String, // Using existing password field for hashed passwords
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct RegisterForm {
    username: String,
    password: String,
}

#[get("/login")]
async fn login_form(user: Option<Identity>, tera: web::Data<Tera>) -> impl Responder {
    if user.is_some() {
        return HttpResponse::Found()
            .append_header(("LOCATION", "/"))
            .finish();
    }

    let mut context = tera::Context::new();
    context.insert("user_status", "Anonymous");

    let rendered = tera.render("login.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[get("/register")]
async fn register_form(user: Option<Identity>, tera: web::Data<Tera>) -> impl Responder {
    if user.is_some() {
        return HttpResponse::Found()
            .append_header(("LOCATION", "/"))
            .finish();
    }

    let mut context = tera::Context::new();
    context.insert("user_status", "Anonymous");

    let rendered = tera.render("register.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[post("/login")]
async fn login(
    req: HttpRequest,
    form: web::Form<LoginForm>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    // Validate username
    if !is_valid_title(&form.username) {
        return HttpResponse::BadRequest().body("Invalid username format");
    }

    // Query user from the database
    let user =
        sqlx::query_as::<_, User>("SELECT username, password FROM users WHERE username = $1")
            .bind(&form.username)
            .fetch_optional(pool.get_ref())
            .await;

    match user {
        Ok(Some(user)) if verify(&form.password, &user.password).unwrap() => {
            Identity::login(&req.extensions(), form.username.clone()).unwrap();
            HttpResponse::Found()
                .append_header(("Location", "/"))
                .finish()
        }
        Ok(Some(_)) => HttpResponse::Unauthorized().body("Invalid password"),
        Ok(None) => HttpResponse::Unauthorized().body("User not found"),
        Err(e) => HttpResponse::InternalServerError().body(format!("Error: {}", e)),
    }
}

#[post("/register")]
async fn register(
    req: HttpRequest,
    form: web::Form<RegisterForm>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    // Validate username
    if !is_valid_title(&form.username) {
        return HttpResponse::BadRequest().body("Invalid username format");
    }

    // Check if the username already exists
    let user_exists = sqlx::query("SELECT 1 FROM users WHERE username = $1")
        .bind(&form.username)
        .fetch_optional(pool.get_ref())
        .await
        .unwrap()
        .is_some();

    if user_exists {
        HttpResponse::Conflict().body("User exists")
    } else {
        // Hash the password before inserting it into the database
        let password_hash = hash(&form.password, DEFAULT_COST).unwrap();

        // Insert the new user into the database
        sqlx::query("INSERT INTO users (username, password) VALUES ($1, $2)")
            .bind(&form.username)
            .bind(&password_hash)
            .execute(pool.get_ref())
            .await
            .unwrap();

        Identity::login(&req.extensions(), form.username.clone()).unwrap();
        HttpResponse::Found()
            .append_header(("Location", "/"))
            .finish()
    }
}

#[get("/logout")]
async fn logout(user: Option<Identity>, tera: web::Data<Tera>) -> impl Responder {
    // Check if the user is logged in, if not redirect to home
    if user.is_none() {
        return HttpResponse::Found()
            .append_header(("LOCATION", "/"))
            .finish();
    }

    let user_status = user
        .and_then(|id| id.id().ok())
        .unwrap_or_else(|| "Anonymous".to_string());

    let mut context = tera::Context::new();
    context.insert("user_status", &user_status);

    // Render the logout confirmation page
    let rendered = tera.render("logout.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[post("/logout")]
async fn logout_api(user: Option<Identity>) -> impl Responder {
    if let Some(user) = user {
        user.logout(); // Log the user out
    }

    // Redirect to home after logging out
    HttpResponse::Found()
        .append_header(("Location", "/"))
        .finish()
}
