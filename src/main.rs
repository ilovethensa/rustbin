use actix_identity::{Identity, IdentityMiddleware};
use actix_session::{SessionMiddleware, storage::CookieSessionStore};
use actix_web::{cookie::Key, App, HttpRequest, HttpResponse, HttpServer, Responder, web, get, post, middleware::Logger};
use actix_files as fs;
use sqlx::postgres::PgPoolOptions;
use sqlx::FromRow;
use tera::Tera;
use actix_web::HttpMessage;
use chrono::NaiveDateTime; // Import chrono
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, FromRow)]
struct User {
    username: String,
    password: String
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String
}

#[derive(Deserialize)]
struct RegisterForm {
    username: String,
    password: String
}

#[derive(Deserialize)]
struct CreatePasteForm {
    title: String,
    content: String
}

#[derive(serde::Serialize)]
struct Paste {
    creator_username: String,
    title: String,
    content: String,
    created_at: NaiveDateTime,
    views: i32,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect("postgres://postgres:postgres@localhost/rustbin")
        .await
        .unwrap();

    sqlx::migrate!().run(&pool).await.unwrap();

    let tera = Tera::new("templates/**/*").unwrap();
    let secret_key = Key::generate();

    HttpServer::new(move || {
        App::new()
            .wrap(IdentityMiddleware::default())
            .wrap(SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone())
                .cookie_secure(false)
                .build())
            .wrap(Logger::default())
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(tera.clone()))
            .service(index)
            .service(login)
            .service(logout)
            .service(logout_api)
            .service(register)
            .service(login_form)
            .service(register_form)
            .service(create_form)
            .service(create_paste)
            .service(view_paste)
            .service(fs::Files::new("/static", "./static").show_files_listing().use_last_modified(true))
    })
    .bind(("127.0.0.1", 8080))?
    .workers(2)
    .run()
    .await
}

#[get("/")]
async fn index(user: Option<Identity>, tera: web::Data<Tera>, pool: web::Data<sqlx::PgPool>) -> impl Responder {
    let user_status = user
        .and_then(|id| id.id().ok())
        .unwrap_or_else(|| "Anonymous".to_string());

    // Fetch pastes from the database and map to the Paste struct
    let pastes = sqlx::query!(
        "SELECT creator_username, title, content, created_at, views FROM pastes"
    )
    .fetch_all(pool.get_ref())
    .await;

    let mut context = tera::Context::new();
    context.insert("user_status", &user_status);

    match pastes {
        Ok(pastes) => {
            // Ensure non-optional values by using `map`
            let pastes: Vec<Paste> = pastes.into_iter()
                .map(|paste| Paste {
                    creator_username: paste.creator_username,
                    title: paste.title,
                    content: paste.content,
                    created_at: paste.created_at.unwrap_or(NaiveDateTime::from_timestamp(0, 0)),
                    views: paste.views.unwrap_or(0),
                })
                .collect();

            context.insert("pastes", &pastes);
        }
        Err(_) => {
            // Insert an empty vector if the query fails
            context.insert("pastes", &Vec::<Paste>::new());
        }
    }

    let rendered = tera.render("index.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[get("/paste/{title}")]
async fn view_paste(title: web::Path<String>, pool: web::Data<sqlx::PgPool>) -> impl Responder {
    let title = title.into_inner(); // Extract the title from the Path

    // Increment view count
    let result = sqlx::query!(
        "UPDATE pastes SET views = views + 1 WHERE title = $1 RETURNING content",
        title
    )
    .fetch_optional(pool.get_ref())
    .await;

    match result {
        Ok(Some(paste)) => HttpResponse::Ok().content_type("text/plain").body(paste.content),
        Ok(None) => HttpResponse::NotFound().body("Paste not found"),
        Err(_) => HttpResponse::InternalServerError().body("Error fetching paste"),
    }
}

#[post("/login")]
async fn login(
    req: HttpRequest,
    form: web::Form<LoginForm>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    // Query user from the database
    let user = sqlx::query_as::<_, User>(
        "SELECT username, password FROM users WHERE username = $1"
    )
    .bind(&form.username)
    .fetch_optional(pool.get_ref())
    .await;

    match user {
        Ok(Some(user)) if user.password == form.password => {
            // Assuming you have a way to handle session management and authentication
            // Here we use Identity to handle login
            Identity::login(&req.extensions(), form.username.clone()).unwrap();

            // Redirect to home page on successful login
            HttpResponse::Found()
                .header("Location", "/")
                .finish()
        }
        Ok(Some(_)) => HttpResponse::Unauthorized().body("Invalid password"),
        Ok(None) => HttpResponse::Unauthorized().body("User not found"),
        Err(e) => HttpResponse::InternalServerError().body(format!("Error: {}", e)),
    }
}


#[post("/register")]
async fn register(
    form: web::Form<RegisterForm>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
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

        // Redirect to login page or another page
        HttpResponse::Found()
            .header("Location", "/login")
            .finish()
    }
}

#[get("/logout")]
async fn logout(user: Option<Identity>, tera: web::Data<Tera>) -> impl Responder {
    // Check if the user is logged in
    let user_status = user
        .and_then(|id| id.id().ok())
        .unwrap_or_else(|| "Anonymous".to_string());

    // Prepare context for the template
    let mut context = tera::Context::new();
    context.insert("user_status", &user_status);

    // Render the confirmation template
    let rendered = tera.render("logout.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}


#[post("/logout")]
async fn logout_api(user: Identity) -> impl Responder {
    user.logout();
    HttpResponse::Found()
        .header("Location", "/")
        .finish()
}

#[get("/login")]
async fn login_form(user: Option<Identity>, tera: web::Data<Tera>) -> impl Responder {
    if user.is_some() {
        return HttpResponse::Found()
            .header("LOCATION", "/")
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
            .header("LOCATION", "/")
            .finish();
    }

    let mut context = tera::Context::new();
    context.insert("user_status", "Anonymous");

    let rendered = tera.render("register.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}



#[get("/create")]
async fn create_form(user: Option<Identity>, tera: web::Data<Tera>) -> impl Responder {
    let user_status = user
        .and_then(|id| id.id().ok())
        .unwrap_or_else(|| "Anonymous".to_string());

    let mut context = tera::Context::new();
    context.insert("user_status", &user_status);

    let rendered = tera.render("create.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}


#[post("/create")]
async fn create_paste(user: Option<Identity>, form: web::Form<CreatePasteForm>, pool: web::Data<sqlx::PgPool>) -> impl Responder {
    let username = user
        .and_then(|id| id.id().ok())
        .ok_or(HttpResponse::Unauthorized().body("Login required"))
        .unwrap();

    let result = sqlx::query("INSERT INTO pastes (creator_username, title, content) VALUES ($1, $2, $3)")
        .bind(&username)
        .bind(&form.title)
        .bind(&form.content)
        .execute(pool.get_ref())
        .await;

    match result {
        Ok(_) => HttpResponse::Created().body("Paste created"),
        Err(sqlx::Error::Database(err)) if err.message().contains("duplicate key value") => {
            HttpResponse::Conflict().body("Paste with this title already exists")
        }
        Err(_) => HttpResponse::InternalServerError().body("Error creating paste"),
    }
}
