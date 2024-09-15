use regex::Regex;

// Helper function to validate the title
pub fn is_valid_title(title: &str) -> bool {
    let re = Regex::new(r"^[a-zA-Z0-9._()]*$").unwrap();
    re.is_match(title)
}
