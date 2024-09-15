use actix_identity::{Identity, IdentityMiddleware};
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{
    cookie::Key, get, middleware::Logger, post, web, App, HttpRequest, HttpResponse,
    HttpServer, Responder,
};
use actix_web::HttpMessage;
use sqlx::postgres::PgPoolOptions;
use sqlx::FromRow;
use std::sync::Arc;
use tera::Tera;
use actix_files as fs;
use sqlx::migrate::Migrator;

// Define a struct to hold user data
#[derive(Debug, Clone, FromRow)]
struct User {
    username: String,
    password: String,
}

// Define a struct to hold login form data
#[derive(serde::Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

// Define a struct to hold register form data
#[derive(serde::Deserialize)]
struct RegisterForm {
    username: String,
    password: String,
}

#[derive(serde::Deserialize)]
struct CreatePasteForm {
    title: String,
    content: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let secret_key = Key::generate();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect("postgres://postgres:postgres@localhost/rustbin").await.unwrap();
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await.unwrap();
    let tera = Tera::new("templates/**/*").unwrap(); // Load templates from the specified directory

    HttpServer::new(move || {
        let session_mw = SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone())
            .cookie_secure(false)
            .build();

        App::new()
            .wrap(IdentityMiddleware::default())
            .wrap(session_mw)
            .wrap(Logger::default())
            .app_data(web::Data::new(pool.clone())) // Pass the database pool to the application
            .app_data(web::Data::new(tera.clone())) // Pass Tera instance to the application
            .service(index)
            .service(login)
            .service(logout)
            .service(register)
            .service(login_form)
            .service(register_form)
            .service(create_form)  // New route for create form
            .service(create_paste) // New route for paste creation
            .service(
                fs::Files::new("/static", "./static")
                    .show_files_listing() // Optional: Enable directory listing
                    .use_last_modified(true) // Optional: Enable Last-Modified headers
            )
    })
    .bind(("127.0.0.1", 8080))?
    .workers(2)
    .run()
    .await
}

#[get("/")]
async fn index(user: Option<Identity>, tera: web::Data<Tera>) -> impl Responder {
    let user_status = if let Some(identity) = user {
        match identity.id() {
            Ok(user_id) => format!("Welcome! {}", user_id),
            Err(_) => "Anonymous".to_owned(), // Handle error case
        }
    } else {
        "Anonymous".to_owned()
    };

    let mut context = tera::Context::new();
    context.insert("user_status", &user_status);

    let rendered = tera.render("index.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[post("/login")]
async fn login(
    request: HttpRequest,
    form: web::Form<LoginForm>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    let username = form.username.clone();
    let password = form.password.clone();

    // Query the user from the database
    let user = sqlx::query_as::<_, User>("SELECT username, password FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(pool.get_ref())
        .await;

    match user {
        Ok(Some(user)) if user.password == password => {
            Identity::login(&request.extensions(), username.clone()).unwrap();
            HttpResponse::Ok().body("Logged in successfully!")
        }
        Ok(Some(_)) => HttpResponse::Unauthorized().body("Invalid password"),
        Ok(None) => HttpResponse::Unauthorized().body("User not found"),
        Err(e) => HttpResponse::InternalServerError().body(format!("Internal server error {}", e)),
    }
}

#[post("/register")]
async fn register(
    form: web::Form<RegisterForm>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    let username = form.username.clone();
    let password = form.password.clone();

    // Check if the user already exists
    let user_exists = sqlx::query("SELECT 1 FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(pool.get_ref())
        .await
        .unwrap()
        .is_some();

    if user_exists {
        HttpResponse::Conflict().body("User already exists")
    } else {
        // Insert the new user into the database
        sqlx::query("INSERT INTO users (username, password) VALUES ($1, $2)")
            .bind(&username)
            .bind(&password)
            .execute(pool.get_ref())
            .await
            .unwrap();
        HttpResponse::Created().body("User created successfully!")
    }
}

#[post("/logout")]
async fn logout(user: Identity) -> impl Responder {
    user.logout();
    HttpResponse::NoContent()
}

#[get("/login")]
async fn login_form(tera: web::Data<Tera>) -> impl Responder {
    let rendered = tera.render("login.html", &tera::Context::new()).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[get("/register")]
async fn register_form(tera: web::Data<Tera>) -> impl Responder {
    let rendered = tera.render("register.html", &tera::Context::new()).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[get("/create")]
async fn create_form(user: Option<Identity>, tera: web::Data<Tera>) -> impl Responder {
    let user_status = if let Some(identity) = user {
        match identity.id() {
            Ok(user_id) => format!("Welcome! {}", user_id),
            Err(_) => "Anonymous".to_owned(), // Handle error case
        }
    } else {
        "Anonymous".to_owned()
    };

    let mut context = tera::Context::new();
    context.insert("user_status", &user_status);

    let rendered = tera.render("create.html", &context)
        .unwrap_or_else(|_| "Error rendering template".to_string());

    HttpResponse::Ok().content_type("text/html").body(rendered)
}


#[post("/create")]
async fn create_paste(
    user: Option<Identity>,
    form: web::Form<CreatePasteForm>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    let identity = match user {
        Some(identity) => identity,
        None => return HttpResponse::Unauthorized().body("You need to be logged in to create a paste."),
    };

    let username = identity.id().unwrap(); // Assuming the identity id is the username
    let title = form.title.clone();
    let content = form.content.clone();

    // Insert the new paste into the database
    let result = sqlx::query("INSERT INTO pastes (creator_username, title, content) VALUES ($1, $2, $3)")
        .bind(&username)
        .bind(&title)
        .bind(&content)
        .execute(pool.get_ref())
        .await;

    match result {
        Ok(_) => HttpResponse::Created().body("Paste created successfully!"),
        Err(e) => HttpResponse::InternalServerError().body(format!("Internal server error: {}", e)),
    }
}
