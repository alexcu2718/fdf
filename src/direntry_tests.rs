
//use crate::direntry::DirEntry;

#[cfg(test)]
mod tests {
   // use super::*;
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
        let _=std::fs::create_dir(&dir_path);
        //throwing the error because of the directory already exists
        
        std::fs::write(dir_path.join("file1.txt"), "test1").unwrap();
        std::fs::write(dir_path.join("file2.txt"), "test2").unwrap();
        let _=std::fs::create_dir(dir_path.join("subdir"));//.unwrap();
        
        let dir_entry = DirEntry::new(dir_path.as_os_str()).unwrap();
        let entries = dir_entry.read_dir().unwrap();
        scopeguard::defer! {
            std::fs::remove_dir_all(&dir_path).unwrap();
        }
        
        assert_eq!(entries.len(), 3);
        let mut names: Vec<_> = entries.iter().map(|e| e.file_name().to_vec()).collect();
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
    fn base_len_test(){
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.as_path().join("testfile.txt");
        std::fs::write(&file_path, "test").unwrap();
        
        let entry:u8 = DirEntry::new(file_path.as_os_str()).unwrap().base_len();
        let std_entry:u8=(std::path::Path::new(file_path.as_os_str()).parent().unwrap().as_os_str().len()+1) as u8 ;
        assert_eq!(entry, std_entry);
    }
    #[test]
    fn test_full_path() {
        let temp_dir = std::env::temp_dir();
        std::env::set_current_dir(&temp_dir).unwrap();
        let file_path = DirEntry::new(".").unwrap().as_full_path().unwrap();
        let my_path:Box<[u8]>=file_path.as_bytes().into();

        let my_path_std:std::path::PathBuf=std::path::Path::new(".").canonicalize().unwrap();
        let bytes_std:&[u8]=my_path_std.as_os_str().as_bytes();
        assert_eq!(&*my_path,bytes_std );

        assert_eq!(file_path.is_dir(),my_path_std.is_dir());



        
    }


   
}

