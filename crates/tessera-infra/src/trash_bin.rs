use std::path::Path;

use tessera_app::ports::Trash;

/// The platform Trash / Recycle Bin. Items stay recoverable (ADR-0007).
pub struct SystemTrash {
    ctx: trash::TrashContext,
}

impl Default for SystemTrash {
    fn default() -> Self {
        #[allow(unused_mut)]
        let mut ctx = trash::TrashContext::default();
        #[cfg(target_os = "macos")]
        {
            // NSFileManager needs no Finder automation (TCC) prompt and fails
            // per item instead of per batch.
            use trash::macos::{DeleteMethod, TrashContextExtMacos};
            ctx.set_delete_method(DeleteMethod::NsFileManager);
        }
        SystemTrash { ctx }
    }
}

impl Trash for SystemTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        self.ctx.delete(path).map_err(|e| e.to_string())
    }
}
