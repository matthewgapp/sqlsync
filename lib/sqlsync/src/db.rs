use rusqlite::{
    hooks::{AuthAction, AuthContext, Authorization},
    Connection, OpenFlags,
};
use sqlite_vfs::FilePtr;

use crate::{
    journal::Journal, page::PAGESIZE, storage::Storage, vfs::StorageVfs,
};

pub struct ConnectionPair {
    pub readwrite: Connection,
    pub readonly: Connection,
}

type Result<T> = std::result::Result<T, rusqlite::Error>;

pub fn open_with_vfs<J: Journal>(
    journal: J,
) -> Result<(ConnectionPair, Box<Storage<J>>)> {
    let mut storage = Box::new(Storage::new(journal));
    let storage_ptr = FilePtr::new(&mut storage);

    // generate random vfs name
    let vfs_name = format!("local-vfs-{}", rand::random::<u64>());

    // register the vfs globally
    let vfs = StorageVfs::new(storage_ptr);
    sqlite_vfs::register(&vfs_name, vfs)
        .expect("failed to register local-vfs with sqlite");

    let sqlite = Connection::open_with_flags_and_vfs(
        "main.db",
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        &vfs_name,
    )?;

    sqlite.pragma_update(None, "page_size", PAGESIZE)?;
    sqlite.pragma_update(None, "synchronous", "off")?;
    sqlite.pragma_update(None, "journal_mode", "memory")?;

    // Enable incremental auto_vacuum support for query subscriptions
    // When SQLite is in incremental auto_vacuum mode, it will maintain
    // ptrmap pages in the database file. These pages allow us to
    // efficiently map changed pages back to their corresponding root.
    sqlite.pragma_update(None, "auto_vacuum", "incremental")?;

    // TODO: benchmark with/without cache
    // sqlite.pragma_update(None, "default_cache_size", 0).unwrap();
    // sqlite.pragma_update(None, "cache_size", 0).unwrap();

    let sqlite_readonly = Connection::open_with_flags_and_vfs(
        "main.db",
        OpenFlags::SQLITE_OPEN_READ_ONLY,
        &vfs_name,
    )?;

    sqlite_readonly.authorizer(Some(|ctx: AuthContext| match ctx.action {
        AuthAction::Select => Authorization::Allow,
        AuthAction::Read { .. } => Authorization::Allow,
        AuthAction::Recursive => Authorization::Allow,
        AuthAction::Function { .. } => Authorization::Allow,
        _ => Authorization::Deny,
    }));

    Ok((
        ConnectionPair { readwrite: sqlite, readonly: sqlite_readonly },
        storage,
    ))
}
