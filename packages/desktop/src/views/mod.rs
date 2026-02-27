mod login;
pub use login::Login;

mod register;
pub use register::Register;

mod sidebar_layout;
pub use sidebar_layout::SidebarLayout;

mod note_detail;
pub use note_detail::NoteDetail;

pub use ui::views::NotesPlaceholder as Notes;

mod settings;
pub use settings::Settings;
