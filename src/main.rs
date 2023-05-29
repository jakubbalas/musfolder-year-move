use fs_extra::dir::{self, create};
use fs_extra::file;
use id3::{Tag, TagLike};
use postgres::{Client, NoTls};
use rand::prelude::*;
use std::env;
use std::result::Result;
use log::{info, warn};
use std::{
    fs::rename,
    path::{Path, PathBuf},
};

struct TopFolder {
    year: i32,
    genre: String,
    colpath: String,
}

const YEAR_INIT : i32 = 9999;
const YEAR_UNKNOWN : i32 = 0;
const YEAR_ERR : i32 = 6666;

fn main() -> Result<(), ()> {
    let mut client = Client::connect(
        "host=localhost user=mmove password=mmove dbname=mmove",
        NoTls,
    )
    .unwrap();
    let args: Vec<String> = env::args().collect();
    let collection_path_str = &args[1];
    match collection_path_str.to_lowercase().find("music") {
        None => {
            println!("{:?} is not a music folder, exiting", collection_path_str);
            return Ok(());
        }
        _ => (),
    }
    let collection_path = Path::new(collection_path_str);
    if !collection_path.exists(){
        println!("Base folder does not exist.");
        return Err(());
    }

    if args.iter().any(|i| i == "--load-folders") {
        step_load_files(collection_path, &mut client);
    } 
    step_load_years(collection_path_str, &mut client);
    step_create_year_genre_folders(collection_path_str, &mut client);
    step_move_items(collection_path_str, &mut client);
    Ok(())
}


fn step_load_files(music_base: &Path, client: &mut Client) {
    music_base.read_dir().unwrap().for_each(|x| {
        let x = x.unwrap();
        let path = x.path();

        if path.is_dir() && path.to_str().unwrap().contains("-q") {
            let mut bits = path.file_name().unwrap().to_str().unwrap().split("-");
            let base_folder_data = TopFolder {
                genre: bits.nth(0).unwrap().to_string(),
                year: bits.nth(0).unwrap().parse::<i32>().unwrap(),
                colpath: music_base.to_str().unwrap().to_string(),
            };

            println!("Going through a folder: {:?}", path);
            folder_load(&path, &base_folder_data, 0, client);
        }
    });
}

fn folder_load(folder: &Path, base_folder_data: &TopFolder, depth_level: i32, client: &mut Client) {
    folder.read_dir().unwrap().for_each(|x| {
        let path = x.unwrap().path();
        if path.is_dir() {
            let next_depth = depth_level + 1;
            let _ = folder_load(&path, base_folder_data, next_depth, client);
        } else if file_is_song(&path) && depth_level == 0{
            let year: i32 = YEAR_INIT;
            client.execute(
                "INSERT INTO foldermove (filepath, depth, year, moved, genre, currentyear, isdir, collection_base) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[&path.to_str(), &depth_level, &year, &false, &base_folder_data.genre, &base_folder_data.year, &false, &base_folder_data.colpath],
            ).unwrap();
        } else if file_is_deletable(&path){
            match std::fs::remove_file(&path) {
                Ok(_) => info!("Removed file: {:?}", path),
                Err(e) => warn!("Error removing file: {:?}, error: {:?}", path, e),
            }
        }
    });
    remove_empty_folders(folder);
    let year: i32 = YEAR_INIT;
    client.execute(
        "INSERT INTO foldermove (filepath, depth, year, moved, genre, currentyear, isdir, collection_base) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &[&folder.to_str(), &depth_level, &year, &false, &base_folder_data.genre, &base_folder_data.year, &true, &base_folder_data.colpath],
    ).unwrap();
}

fn step_load_years(col_path: &String, client: &mut Client) {
    println!("Now lets load some years into the database!");
    let query = "SELECT filepath FROM foldermove WHERE year = $1 and collection_base = $2";
    let rows = client.query(query, &[&YEAR_INIT, &col_path]).unwrap();
    let mut year;
    for row in rows {
        let file_path_str: String = row.get(0);
        let file_path = Path::new(&file_path_str);
        if !file_path.exists() {
            println!("file doesn't exist: {:?}", file_path);
            year = YEAR_ERR;
        } else if file_path.is_dir() {
            year = get_folder_year(file_path);
        } else {
            year = get_song_year(&file_path);
        }
        let query = "UPDATE foldermove SET year = $1 WHERE filepath = $2";
        client.execute(query, &[&year, &file_path_str]).unwrap();
    }
}

fn step_create_year_genre_folders(collection_base: &String, client: &mut Client) {
    let q = "SELECT year, genre, collection_base FROM foldermove WHERE moved = False AND year BETWEEN 1800 AND 2500 AND currentyear != year AND collection_base=$1 GROUP BY year, genre, collection_base;";
    let rows = client.query(q, &[&collection_base]).unwrap();
    for row in rows {
        let year: i32 = row.get(0);
        let genre: String = row.get(1);
        let collection_path_str: String = row.get(2);
        let collection_path = Path::new(&collection_path_str);
        make_base_year_genre_folder(&genre, &year, &collection_path);
    }
}

fn step_move_items(collection_base: &String, client: &mut Client) {
    println!("Running step of the move");
    let q = "SELECT filepath, year, genre, collection_base FROM foldermove WHERE moved = False AND year between 1 and 3000 and currentyear != year AND collection_base=$1 order by depth desc;";
    let rows = client.query(q, &[&collection_base]).unwrap();
    for row in rows{
        let itempath: String = row.get(0);
        let item = Path::new(&itempath);
        let year: i32 = row.get(1);
        let genre: String = row.get(2);
        let collection_base_path: String = row.get(3);
        if !item.exists() {
            warn!("File doesn't exist: {:?}", item);
            continue
        };
        if item.is_dir() {
            remove_empty_folders(item);
            let mut can_be_moved = true;
            item.read_dir().unwrap().for_each(|x| {
                let x = x.unwrap();
                let path = x.path();
                if path.is_dir() {
                    can_be_moved = false;
                }
            });
            if !can_be_moved {
                continue;
            }
        }
        let genrefolder = construct_genre_year_folder(&genre, &year, Path::new(&collection_base_path));
        match safe_move_item(item, &genrefolder) {
            Ok(_) => {
                let u = "UPDATE foldermove SET moved = True WHERE filepath = $1;";
                client.execute(u, &[&itempath]).unwrap();        
            },
            Err(e) => println!("{:?}", e),
        };
    }
}

fn make_base_year_genre_folder(genre: &str, year: &i32, music_base: &Path) -> PathBuf {
    let yearfolder = construct_genre_year_folder(genre, year, music_base);
    if !yearfolder.exists() {
        dir::create(&yearfolder, false).unwrap();
        println!("created folder: {:?}", yearfolder);
    }
    return yearfolder;
}

fn construct_genre_year_folder(genre: &str, year: &i32, base: &Path) -> PathBuf {
    base.join(Path::new(&format!("{}-{}", genre, year)))
}

fn remove_empty_folders(folder: &Path) {
    folder.read_dir().unwrap().for_each(|x| {
        let subpath = x.unwrap().path();
        if subpath.is_dir() {
            let mut empty = true;
            subpath.read_dir().unwrap().for_each(|_| {
                empty = false;
            });
            if empty {
                println!("deleting empty folder: {:?}", subpath);
                match std::fs::remove_dir(&subpath) {
                    Ok(_) => info!("Deleted folder: {:?}", subpath),
                    Err(_) => warn!("Couldn't delete folder: {:?}", subpath)
                }
            }
        }
    });
}

fn get_folder_year(folder_path: &Path) -> i32 {
    folder_path
        .read_dir()
        .unwrap()
        .map(|x| get_song_year(&x.unwrap().path()))
        .max()
        .unwrap_or_default()  
}

fn get_song_year(song_path: &Path) -> i32 {
    if !file_is_song(&song_path) {
        return 0;
    }
    let tag_read = Tag::read_from_path(&song_path.to_str().unwrap());
    match tag_read {
        Ok(_) => (),
        Err(_) => {
            println!("no tag found in file: {:?}", song_path);
            return YEAR_UNKNOWN;
        }        
    }
    let tag = tag_read.unwrap();
    match tag.year() {
        Some(year) => return year,
        None => (),
    }

    let yeartagopt = tag.get("TDRC").and_then(|frame| frame.content().text());
    match yeartagopt {
        Some(yeartag) => {
            if yeartag.matches("-").count() > 0 && yeartag.matches("-").count() != 2{
                return YEAR_UNKNOWN;
            }
            else if yeartag.contains("-") {
                println!("{:?}", yeartag);
                return yeartag.split("-").next().unwrap().parse::<i32>().unwrap()    
            } else{
                return yeartag.parse::<i32>().unwrap();
            }
        },
        None => {
            println!("no year tag found in file: {:?}", song_path);
            return YEAR_UNKNOWN;
        }
    }
}

fn file_is_song(path: &Path) -> bool {
    let ext = path.extension().unwrap_or_default();
    match ext.to_str().unwrap() {
        "mp3" => true,
        "flac" => true,
        "ogg" => true,
        _ => false,
    }
}

fn file_is_deletable(path: &Path) -> bool {
    let filename = path.file_name().unwrap().to_str().unwrap();
    match filename {
        ".DS_Store" => return true,
        _ => (),
    };

    let ext = path.extension().unwrap_or_default();
    match ext.to_str().unwrap() {
        "jpg" => true,
        "jpeg" => true,
        "png" => true,
        "txt" => true,
        "nfo" => true,
        "m3u" => true,
        _ => false,
    }
}

fn safe_move_item(from: &Path, to: &Path) -> Result<String, String>{
    if !to.exists() {
        panic!("Destination base folder doesn't exist")
    }
    if from.is_dir() {
        let folder_name = from.file_name().unwrap();
        let mut move_path = to.join(folder_name);
        
        if move_path.exists() {
            println!("Found duplicate for folder: {:?}", move_path);
            let randomized_path = to.join(Path::new(&format!(
                "{}-{}",
                folder_name.to_str().unwrap(),
                random::<u32>()
            )));
            move_path = randomized_path;
        }
        create(&move_path, false).unwrap();
        match rename(from, &move_path) {
            Ok(_) => return Ok(move_path.to_str().unwrap().to_string()),
            Err(e) => return Err(format!("Something went wrong during folder rename: {:?}", e).to_string()),
        };
    } else {
        let filename = from.file_name().unwrap();
        let move_path = to.join(filename);
        if move_path.exists() {
            println!("Found duplicate for file: {:?}", move_path);
            let randomized_name = from.parent().unwrap().join(Path::new(&format!(
                "{}-{}.{}",
                from.file_stem().unwrap().to_str().unwrap(),
                random::<u32>(),
                from.extension().unwrap_or_default().to_str().unwrap()
            )));

            match rename(from, &randomized_name) {
                Ok(_) => (),
                Err(e) => return Err(format!("Something went wrong during file rename: {:?}", e).to_string()),
            };
            let mut copy_options = file::CopyOptions::new();
            copy_options.overwrite = false;
            match file::move_file(&randomized_name, &to.join(Path::new(&randomized_name.file_name().unwrap())), &copy_options) {
                Ok(_) => return Ok(randomized_name.to_str().unwrap().to_string()),
                Err(e) => return Err(format!("Something went wrong during randomised file move: {:?}", e).to_string()),                
            };
        } else {
            let mut copy_options = file::CopyOptions::new();
            copy_options.overwrite = false;
            match file::move_file(from, to.join(from.file_name().unwrap()), &copy_options) {
                Ok(_) => return Ok(from.to_str().unwrap().to_string()),
                Err(e) => return Err(format!("Something went wrong during file move: {:?}", e).to_string()),                
            };
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movingfolder() {
        let base_name = "tstfolder".to_string();
        let base = Path::new(&base_name);
        let origin = base.join(Path::new("origin"));
        assert!(origin.exists());
        let newtest = base.join(Path::new("testres"));
        let mut copyopt = dir::CopyOptions::new();
        copyopt.copy_inside = true;
        dir::create(&newtest, true).unwrap();
        dir::copy(origin, &newtest, &copyopt).unwrap();
        let basemus = newtest.join(Path::new("origin"));
        let mut client = Client::connect(
            "host=localhost user=mmove password=mmove dbname=mmove",
            NoTls,
        )
        .unwrap();
        step_load_files(&basemus, &mut client);
        let col_id = "tstfolder/testres/origin".to_string();
        step_load_years(&col_id, &mut client);
        step_create_year_genre_folders(&col_id, &mut client);
        step_move_items(&col_id, &mut client);
        assert!(newtest.exists());
    }
}
