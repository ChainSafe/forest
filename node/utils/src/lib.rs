/// Get file count in a certain directory
/// Will return the number of files in the directory
///
/// Error will be logged if:
/// - The provided path doesn't exist.
/// - The process lacks permissions to view the contents.
/// - The path points at a non-directory file.
///
/// # Example
/// ```
/// use utils::count_files;
/// use utils::get_home_dir;
///
/// let path_to_keystore = get_home_dir() + "/.forest/libp2p/keypair";
/// let dir_to_keystore = path_to_keystore.replace("/keypair", "");
/// match count_files(&dir_to_keystore) {
///     Err(e) => {
///         info!(log, "Error {:?}", &e);
///     }
///     Ok(v) => {
///     fs::rename(
///             path_to_keystore.clone(),
///             path_to_keystore.clone() + &format!(".old({:})", v),
///         );
///     }
/// }
/// ```
pub fn count_files(dir: &str) -> Result<usize> {
    Ok(read_dir(dir)?.enumerate().count())
}

pub fn rename_file(from: &str, to: &str) -> Result<()> {
    rename(from, to)?;
    Ok(())
}
