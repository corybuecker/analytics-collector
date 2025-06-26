use uuid::Uuid;

pub fn generate_uuid_v4() -> String {
    Uuid::new_v4().to_string()
}

pub fn get_environment_variable_with_default<T>(key: &str, default: T) -> T
where
    T: From<String>,
{
    let value = std::env::var(key);

    match value {
        Ok(val) => val.into(),
        Err(_) => default,
    }
}
