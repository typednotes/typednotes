mod login;
pub use login::Login;

mod register;
pub use register::Register;

mod notes;
pub use notes::Notes;

mod note_detail;
pub use note_detail::NoteDetail;

mod settings;
pub use settings::Settings;

mod sidebar_layout;
pub use sidebar_layout::SidebarLayout;

pub(crate) fn make_repo() -> store::Repository<impl store::ObjectStore> {
    #[cfg(target_arch = "wasm32")]
    {
        store::Repository::new(store::IdbStore::new())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        store::Repository::new(store::MemoryStore::new())
    }
}
