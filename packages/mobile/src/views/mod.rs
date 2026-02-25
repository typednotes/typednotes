mod notes;
pub use notes::Notes;

mod note_detail;
pub use note_detail::NoteDetail;

mod settings;
pub use settings::Settings;

mod sidebar_layout;
pub use sidebar_layout::SidebarLayout;

pub(crate) fn make_repo() -> store::Repository<impl store::ObjectStore> {
    store::Repository::new(store::MemoryStore::new())
}
