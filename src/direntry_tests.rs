#[cfg(test)]
mod tests {
    use crate::debug_print;
    use crate::direntry::DirEntry;
    use std::os::unix::ffi::OsStrExt;
  

    #[test]
    fn test_path_methods() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.as_path().join("parent/child.txt");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "test").unwrap();

        let entry = DirEntry::new(file_path.as_os_str()).unwrap();
        assert_eq!(entry.file_name(), b"child.txt");
        assert_eq!(entry.extension().unwrap(), b"txt");
        assert_eq!(
            entry.parent(),
            file_path.parent().unwrap().as_os_str().as_bytes()
        );
    }

    #[test]
    fn test_read_dir() {
        let temp_dir = std::env::temp_dir();
        let dir_path = temp_dir.as_path().join("testdir");
        let _ = std::fs::create_dir(&dir_path);
        //throwing the error because of the directory already exists

        std::fs::write(dir_path.join("file1.txt"), "test1").unwrap();
        std::fs::write(dir_path.join("file2.txt"), "test2").unwrap();
        let _ = std::fs::create_dir(dir_path.join("subdir")); //.unwrap();

        let dir_entry = DirEntry::new(dir_path.as_os_str()).unwrap();
        let entries = dir_entry.read_dir().unwrap();

        let mut names: Vec<_> = entries.iter().map(|e| e.file_name().to_vec()).collect();
        let _ = std::fs::remove_dir_all(&dir_path).unwrap();
        assert_eq!(entries.len(), 3);

        names.sort();
        assert_eq!(
            names,
            vec![
                b"file1.txt".to_vec(),
                b"file2.txt".to_vec(),
                b"subdir".to_vec()
            ]
        );

        for entry in entries {
            assert_eq!(entry.depth(), 1);
            assert_eq!(entry.base_len() as usize, dir_path.as_os_str().len() + 1);
        }
    }

    #[test]
    fn test_hidden_files() {
        let temp_dir = std::env::temp_dir();
        let hidden_file = temp_dir.as_path().join(".hidden");
        std::fs::write(&hidden_file, "").unwrap();

        let entry = DirEntry::new(hidden_file.as_os_str()).unwrap();
        assert!(entry.is_hidden());

        let non_hidden = temp_dir.as_path().join("visible");
        std::fs::write(&non_hidden, "").unwrap();
        let entry = DirEntry::new(non_hidden.as_os_str()).unwrap();
        assert!(!entry.is_hidden());
    }

    #[test]
    fn filename_test() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.as_path().join("testfile.txt");
        std::fs::write(&file_path, "test").unwrap();

        let entry = DirEntry::new(file_path.as_os_str()).unwrap();
        assert_eq!(entry.file_name(), b"testfile.txt");
    }
    #[test]
    fn base_len_test() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.as_path().join("testfile.txt");
        std::fs::write(&file_path, "test").unwrap();

        let entry: u8 = DirEntry::new(file_path.as_os_str()).unwrap().base_len();
        let std_entry: u8 = (std::path::Path::new(file_path.as_os_str())
            .parent()
            .unwrap()
            .as_os_str()
            .len()
            + 1) as u8;
        assert_eq!(entry, std_entry);
    }
    #[test]
    fn test_full_path()->Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir().join("test_full_path");
        std::fs::create_dir_all(&temp_dir).unwrap();

        std::env::set_current_dir(&temp_dir).unwrap();
        let file_path = DirEntry::new(".")?.as_full_path()?;
        debug_print!(&file_path);
        let my_path: Box<[u8]> = file_path.as_bytes().into();

        let my_path_std: std::path::PathBuf = std::path::Path::new(".").canonicalize()?;
        let bytes_std: &[u8] = my_path_std.as_os_str().as_bytes();
        assert_eq!(&*my_path, bytes_std);

        assert_eq!(file_path.is_dir(), my_path_std.is_dir());
        Ok(())
    }

    use std::env::temp_dir;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_iterator() -> core::result::Result<(), Box<dyn std::error::Error>> {
        // make a unique test directory inside temp_dir
        let unique_id = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
        let dir_path: PathBuf = temp_dir().join(format!("test_dir_{}", unique_id));
        fs::create_dir(&dir_path)?;

        // create test files and subdirectory
        fs::write(dir_path.join("file1.txt"), "content")?;
        fs::write(dir_path.join("file2.txt"), "content")?;
        fs::create_dir(dir_path.join("subdir"))?;

        // lean up automatically

        // init a DirEntry for testing
        let dir_entry = DirEntry::new(&dir_path)?;

        // get iterator
        let mut iter = dir_entry.as_iter()?;

        // collect entries
        let mut entries = Vec::new();
        while let Some(entry) = iter.next() {
            entries.push(entry);
        }

        // verify results
        let entry_iter = entries.iter().collect::<Vec<_>>();
        let _ = fs::remove_dir_all(dir_entry.as_path());
        assert_eq!(entries.len(), 3, "Should find two files and one subdir");
        assert!(
            entry_iter
                .clone()
                .iter()
                .any(|e| e.file_name() == b"file1.txt"),
            "Should find file1.txt"
        );
        assert!(
            entry_iter.clone().iter().filter(|e| e.is_dir()).count() == 1,
            "Should find one directory"
        );
        assert!(
            entry_iter
                .clone()
                .iter()
                .filter(|e| e.is_regular_file())
                .count()
                == 2,
            "Should find two regular files"
        );

        Ok(())
    }

    #[test]
    fn test_handles_various_tests() -> Result<(), Box<dyn std::error::Error>> {
        // create empty directory
        let tdir = temp_dir().join("NOTAREALPATHLALALALALA");
        fs::create_dir_all(&tdir)?;

        let dir_entry = DirEntry::new(&tdir)?;

        //PAY ATTENTION TO THE ! MARKS, HARD TO FUCKING SEE
        assert_eq!(
            dir_entry.parent(),
            tdir.parent().unwrap().as_os_str().as_bytes()
        );
        assert_eq!(dir_entry.as_bytes(), tdir.as_os_str().as_bytes());
        assert_eq!(dir_entry.as_path(), &tdir);
        assert!(dir_entry.is_dir(), "Should be a directory");
        assert!(dir_entry.is_empty(), "Directory should be empty");
        assert!(dir_entry.exists(), "Directory should exist");
        assert!(dir_entry.is_readable(), "Directory should be readable");
        assert!(dir_entry.is_writable(), "Directory should be writable");
        assert!(
            !dir_entry.is_executable(),
            "Directory should be not executable"
        );
        assert!(!dir_entry.is_hidden(), "Directory should be not hidden");
        assert!(!dir_entry.is_symlink(), "Directory should be not symlink");

        // Get iterator
        let mut iter = dir_entry.as_iter()?;
        let evaludated_statement = iter.next();
        let _ = fs::remove_dir_all(&tdir);
        // should return no entries (excluding . and ..)
        assert!(
            evaludated_statement.is_none(),
            "Empty directory should have no entries"
        );

        Ok(())
    }
    #[test]
    fn test_dirname() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.as_path().join("parent/child.txt");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "test").unwrap();

        let entry = DirEntry::new(file_path.as_os_str()).unwrap();
        assert_eq!(entry.dirname(), b"parent");
    }
}
