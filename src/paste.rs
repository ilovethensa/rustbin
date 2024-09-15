use actix_web::{get, post, web, HttpResponse, Responder};
use serde::Deserialize;
use tera::Tera;

#[derive(Deserialize)]
struct CreatePasteForm {
    title: String,
    content: String,
}

#[derive(serde::Serialize, Debug)]
struct Paste {
    creator_username: String,
    title: String,
    content: String,
    created_at: i64, // Unix timestamp
    views: i32,
}

#[get("/")]
pub async fn index(
    user: Option<actix_identity::Identity>,
    tera: web::Data<Tera>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    let user_status = user
        .and_then(|id| id.id().ok())
        .unwrap_or_else(|| "Anonymous".to_string());

    // Fetch pastes from the database using query!
    let pastes_result =
        sqlx::query!("SELECT creator_username, title, content, created_at, views FROM pastes")
            .fetch_all(pool.get_ref())
            .await;

    let mut context = tera::Context::new();
    context.insert("user_status", &user_status);
    let default_datetime = 0; // Default Unix timestamp value

    match pastes_result {
        Ok(rows) => {
            let pastes: Vec<Paste> = rows
                .into_iter()
                .map(|row| Paste {
                    creator_username: row.creator_username,
                    title: row.title,
                    content: row.content,
                    created_at: row.created_at.unwrap_or(default_datetime) as i64,
                    views: row.views.unwrap_or(0),
                })
                .collect();

            context.insert("pastes", &pastes);
            println!("Pastes fetched: {:#?}", &pastes); // Log the results
        }
        Err(e) => {
            eprintln!("Error fetching pastes: {:?}", e); // Log the error
            context.insert("pastes", &Vec::<Paste>::new());
        }
    }

    let rendered = tera.render("index.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[get("/paste/{title}")]
pub async fn view_paste(title: web::Path<String>, pool: web::Data<sqlx::PgPool>) -> impl Responder {
    let title = title.into_inner(); // Extract the title from the Path

    // Increment view count
    let result = sqlx::query!(
        "UPDATE pastes SET views = views + 1 WHERE title = $1 RETURNING content",
        title
    )
    .fetch_optional(pool.get_ref())
    .await;

    match result {
        Ok(Some(paste)) => HttpResponse::Ok()
            .content_type("text/plain")
            .body(paste.content),
        Ok(None) => HttpResponse::NotFound().body("Paste not found"),
        Err(_) => HttpResponse::InternalServerError().body("Error fetching paste"),
    }
}

#[get("/create")]
pub async fn create_form(
    user: Option<actix_identity::Identity>,
    tera: web::Data<Tera>,
) -> impl Responder {
    let user_status = user
        .and_then(|id| id.id().ok())
        .unwrap_or_else(|| "Anonymous".to_string());

    let mut context = tera::Context::new();
    context.insert("user_status", &user_status);

    let rendered = tera.render("create.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[post("/create")]
pub async fn create_paste(
    user: Option<actix_identity::Identity>,
    form: web::Form<CreatePasteForm>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    let username = user
        .and_then(|id| id.id().ok())
        .ok_or(HttpResponse::Unauthorized().body("Login required"))
        .unwrap();

    let result =
        sqlx::query("INSERT INTO pastes (creator_username, title, content) VALUES ($1, $2, $3)")
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
