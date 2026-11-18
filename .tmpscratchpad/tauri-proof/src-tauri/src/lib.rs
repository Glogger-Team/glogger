#[tauri::command]
fn registered_cmd() {}

#[tauri::command]
fn unregistered_cmd() {}

#[tauri::command]
fn create_price_helper_quote() {}

pub fn run() {
    tauri::generate_handler![registered_cmd]
}
