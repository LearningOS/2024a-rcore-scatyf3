//! Building applications linker
//! 这里算是链接器，给app增加要素，链接一些东西...

use std::fs::{read_dir, File};
use std::io::{Result, Write};

fn main() {
    println!("cargo:rerun-if-changed=../user/src/");
    println!("cargo:rerun-if-changed={}", TARGET_PATH);
    insert_app_data().unwrap();
}

static TARGET_PATH: &str = "../user/build/elf/";

/// get app data and build linker
/// 在实现 exec 系统调用的时候，我们需要根据应用的名字而不仅仅是一个编号来获取应用的 ELF 格式数据。 
/// 因此，在链接器 os/build.rs 中，我们按顺序保存链接进来的每个应用的名字
fn insert_app_data() -> Result<()> {
    let mut f = File::create("src/link_app.S").unwrap();
    let mut apps: Vec<_> = read_dir("../user/build/elf/")
        .unwrap()
        .into_iter()
        .map(|dir_entry| {
            let mut name_with_ext = dir_entry.unwrap().file_name().into_string().unwrap();
            name_with_ext.drain(name_with_ext.find('.').unwrap()..name_with_ext.len());
            name_with_ext
        })
        .collect();
    apps.sort();

    writeln!(
        f,
        r#"
    .align 3
    .section .data
    .global _num_app
_num_app:
    .quad {}"#,
        apps.len()
    )?;

    for i in 0..apps.len() {
        writeln!(f, r#"    .quad app_{}_start"#, i)?;
    }
    writeln!(f, r#"    .quad app_{}_end"#, apps.len() - 1)?;
    // 各个应用的名字通过 .string 伪指令放到数据段中
    // 链接器会自动在每个字符串的结尾加入分隔符 \0 ，它们的位置由全局符号 _app_names 指出
    // aka对每个应用，在最前面的数据段加一段_app_names，便于os读取，从而可以用_app_names区分ELF?
    writeln!(
        f,
        r#"
    .global _app_names
_app_names:"#
    )?;
    for app in apps.iter() {
        writeln!(f, r#"    .string "{}""#, app)?;
    }

    for (idx, app) in apps.iter().enumerate() {
        println!("app_{}: {}", idx, app);
        writeln!(
            f,
            r#"
    .section .data
    .global app_{0}_start
    .global app_{0}_end
    .align 3
app_{0}_start:
    .incbin "{2}{1}.elf"
app_{0}_end:"#,
            idx, app, TARGET_PATH
        )?;
    }
    Ok(())
}
