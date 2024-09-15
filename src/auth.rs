use actix_identity::Identity;
use actix_web::{get, post, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use serde::Deserialize;
use sqlx::FromRow;
use tera::Tera;

#[derive(Debug, Clone, FromRow)]
struct User {
    password: String,
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
    // Query user from the database
    let user = sqlx::query_as::<_, User>("SELECT password FROM users WHERE username = $1")
        .bind(&form.username)
        .fetch_optional(pool.get_ref())
        .await;

    match user {
        Ok(Some(user)) if user.password == form.password => {
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
async fn register(form: web::Form<RegisterForm>, pool: web::Data<sqlx::PgPool>) -> impl Responder {
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
        // Insert the new user into the database
        sqlx::query("INSERT INTO users (username, password) VALUES ($1, $2)")
            .bind(&form.username)
            .bind(&form.password)
            .execute(pool.get_ref())
            .await
            .unwrap();

        HttpResponse::Found()
            .append_header(("Location", "/login"))
            .finish()
    }
}

#[get("/logout")]
async fn logout(user: Option<Identity>, tera: web::Data<Tera>) -> impl Responder {
    let user_status = user
        .and_then(|id| id.id().ok())
        .unwrap_or_else(|| "Anonymous".to_string());

    let mut context = tera::Context::new();
    context.insert("user_status", &user_status);

    let rendered = tera.render("logout.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[post("/logout")]
async fn logout_api(user: Identity) -> impl Responder {
    user.logout();
    HttpResponse::Found()
        .append_header(("Location", "/"))
        .finish()
}
