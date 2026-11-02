//! chcsv — execute SQL against Oracle and dump SELECT results as CSV.
//!
//! Rust port of the classic Pro*C tool "chcsv" (Copyright (C) 1995 Batayan,
//! GPL). Command-line behavior follows the original as documented in its
//! README/source; intentional improvements are listed in README.md.

mod dpi;

use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::os::raw::c_char;

use dpi::{
    ContextHandle, Conn, Data, QueryInfo, Stmt, Var, MODE_EXEC_DEFAULT, NATIVE_TYPE_BYTES,
    ORACLE_TYPE_VARCHAR, SQLT_BLOB, SQLT_CLOB, SQLT_DAT, SQLT_LNG, SQLT_LNGRAW, SQLT_NCLOB,
    SQLT_NUM, SQLT_RDD,
};

const VERSION: f32 = 2.1;

struct Options {
    outfile: Option<Vec<u8>>,
    append: bool,
    infile: Option<Vec<u8>>,
    enclose: u8,
    terminate: u8,
    longsize: u32,
    fetch_array: u32,
    out_records: u32,
    vertical: bool,
    header: bool,
    nodata_exit: bool,
    consts: Vec<Vec<u8>>,
}

fn usage_err(prog: &[u8]) -> ! {
    let prog = String::from_utf8_lossy(prog);
    eprint!(
        "{prog} Version {VERSION:.2}, Copyright (C) 1995 Batayan.\n\
         Usage: {prog} userid/passwd\n\
         \t[-o Output Filename]\n\
         \t[-a Appended Output Filename]\n\
         \t[-i Input Filename]\n\
         \t[-e Enclosure]\n\
         \t[-t Terminater]\n\
         \t[-l Length of Long Type]\n\
         \t[-f Array size of fetch]\n\
         \t[-b Buffer size of output]\n\
         \t[-v] [-h] [-n]\n\
         \t[const] [const] ....\n\
         *********************************************************************\n\
         \x20   chcsv is distributed under the GNU General Public License.\n\
         \x20   chcsv comes with ABSOLUTELY NO WARRANTY.\n\
         \x20   This is free software, and you are welcome to redistribute it\n\
         \x20   under certain conditions, GPL.\n\
         *********************************************************************\n"
    );
    std::process::exit(1);
}

fn atoi(b: &[u8]) -> u32 {
    let s = String::from_utf8_lossy(b);
    s.trim().chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

/// Option value: glued to the flag ("-f100") or the raw next argv token
/// (even if it starts with '-'), exactly like Option.pc.
fn opt_value<'a>(prog: &[u8], argv: &'a [Vec<u8>], i: &mut usize, inline: &'a [u8]) -> Vec<u8> {
    if !inline.is_empty() {
        *i += 1;
        return inline.to_vec();
    }
    if *i + 1 < argv.len() {
        *i += 2;
        return argv[*i - 1].clone();
    }
    usage_err(prog)
}

/// Mirrors Option() in Option.pc, including its quirks: "-a" falls through
/// to "-o" (so "-a FILE" means append to FILE), and option parsing treats
/// any non-dash token as a positional bind constant.
fn parse_options(prog: &[u8], argv: &[Vec<u8>]) -> Options {
    let mut o = Options {
        outfile: None,
        append: false,
        infile: None,
        enclose: 0,
        terminate: b',',
        longsize: 1000,
        fetch_array: 100,
        out_records: 100,
        vertical: false,
        header: false,
        nodata_exit: false,
        consts: Vec::new(),
    };
    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        if a.first() != Some(&b'-') || a.len() < 2 {
            o.consts.push(a.clone());
            i += 1;
            continue;
        }
        let flag = a[1];
        let inline = &a[2..];
        match flag {
            b'h' => {
                o.header = true;
                i += 1;
            }
            b'f' => {
                let v = opt_value(prog, argv, &mut i, inline);
                o.fetch_array = atoi(&v);
            }
            b'b' => {
                let v = opt_value(prog, argv, &mut i, inline);
                o.out_records = atoi(&v);
            }
            b'l' => {
                let v = opt_value(prog, argv, &mut i, inline);
                o.longsize = atoi(&v);
            }
            b'a' => {
                // falls through to -o in the original: -a FILE appends to FILE
                o.append = true;
                let v = opt_value(prog, argv, &mut i, inline);
                o.outfile = Some(v);
            }
            b'o' => {
                let v = opt_value(prog, argv, &mut i, inline);
                o.outfile = Some(v);
            }
            b'i' => {
                let v = opt_value(prog, argv, &mut i, inline);
                o.infile = Some(v);
            }
            b't' => {
                let v = opt_value(prog, argv, &mut i, inline);
                if v.is_empty() {
                    usage_err(prog);
                }
                o.terminate = v[0];
            }
            b'e' => {
                let v = opt_value(prog, argv, &mut i, inline);
                if v.is_empty() {
                    usage_err(prog);
                }
                o.enclose = v[0];
            }
            b'v' => {
                o.vertical = true;
                i += 1;
            }
            b'n' => {
                o.nodata_exit = true;
                i += 1;
            }
            _ => usage_err(prog),
        }
    }
    if o.vertical {
        o.terminate = b'\n';
    }
    o
}

fn main() {
    let raw: Vec<Vec<u8>> = std::env::args_os().map(|a| a.into_encoded_bytes()).collect();
    if raw.len() < 2 {
        usage_err(&raw[0]);
    }
    let login = raw[1].clone();
    let mut opt = parse_options(&raw[0], &raw[2..]);
    if opt.fetch_array == 0 {
        opt.fetch_array = 1;
    }
    if opt.out_records == 0 {
        opt.out_records = 1;
    }

    let ctx = ContextHandle::new();

    // Split user/pass[@connect] the way Pro*C CONNECT :logname does
    // (first '/', then first '@').
    let (user, pass, dsn) = split_login(&login);

    let sql = read_sql(&opt);

    unsafe { run(&ctx, user, pass, dsn, &sql, &mut opt) }
}

fn split_login(login: &[u8]) -> (&[u8], &[u8], &[u8]) {
    let (user, rest) = match login.iter().position(|&c| c == b'/') {
        Some(p) => (&login[..p], &login[p + 1..]),
        None => (login, &[][..]),
    };
    match rest.iter().position(|&c| c == b'@') {
        Some(p) => (user, &rest[..p], &rest[p + 1..]),
        None => (user, rest, &[][..]),
    }
}

fn read_sql(opt: &Options) -> Vec<u8> {
    let mut sql = Vec::new();
    match &opt.infile {
        Some(name) => {
            let path = String::from_utf8_lossy(name).into_owned();
            let mut f = match File::open(&path) {
                Ok(f) => f,
                Err(_) => {
                    eprintln!("chcsv: {path} Error");
                    std::process::exit(1);
                }
            };
            f.read_to_end(&mut sql).unwrap();
        }
        None => {
            std::io::stdin().read_to_end(&mut sql).unwrap();
        }
    }
    while sql.last().is_some_and(|c| c.is_ascii_whitespace()) {
        sql.pop();
    }
    if sql.last() == Some(&b';') {
        sql.pop();
    }
    sql
}

unsafe fn run(
    ctx: &ContextHandle,
    user: &[u8],
    pass: &[u8],
    dsn: &[u8],
    sql: &[u8],
    opt: &mut Options,
) -> ! {
    // connect
    let mut conn: *mut Conn = std::ptr::null_mut();
    let rc = dpi::dpiConn_create(
        ctx.0,
        user.as_ptr() as *const c_char,
        user.len() as u32,
        pass.as_ptr() as *const c_char,
        pass.len() as u32,
        dsn.as_ptr() as *const c_char,
        dsn.len() as u32,
        std::ptr::null(),
        std::ptr::null(),
        &mut conn,
    );
    if rc != 0 {
        ctx.fail("LOGIN", rc);
    }

    // prepare
    let mut stmt: *mut Stmt = std::ptr::null_mut();
    let rc = dpi::dpiConn_prepareStmt(
        conn,
        0,
        sql.as_ptr() as *const c_char,
        sql.len() as u32,
        std::ptr::null(),
        0,
        &mut stmt,
    );
    if rc != 0 {
        ctx.fail("PREPARE", rc);
    }

    // bind variables (positional consts, interactive prompt fallback)
    bind_variables(ctx, conn, stmt, opt);

    // execute
    let mut num_cols: u32 = 0;
    let rc = dpi::dpiStmt_execute(stmt, MODE_EXEC_DEFAULT, &mut num_cols);
    if rc != 0 {
        ctx.fail("OPEN", rc);
    }

    if num_cols == 0 {
        // non-SELECT: the original executes it silently and commits on logout
        if opt.nodata_exit {
            dpi::dpiStmt_release(stmt);
            dpi::dpiConn_rollback(conn);
            dpi::dpiConn_release(conn);
            std::process::exit(100);
        }
        dpi::dpiConn_commit(conn);
        dpi::dpiStmt_release(stmt);
        dpi::dpiConn_release(conn);
        std::process::exit(0);
    }

    // describe + define every column as a string (same trick as the original:
    // Oracle performs the type->ASCII conversion exactly like SQL*Plus/chcsv)
    let ncols = num_cols as usize;
    let mut names: Vec<Vec<u8>> = Vec::with_capacity(ncols);
    let mut is_number: Vec<bool> = Vec::with_capacity(ncols);
    let mut vars: Vec<*mut Var> = vec![std::ptr::null_mut(); ncols];
    let mut datas: Vec<*mut Data> = vec![std::ptr::null_mut(); ncols];
    let mut row_bytes: usize = ncols * 3;
    for pos in 1..=num_cols {
        let mut qi = std::mem::zeroed::<QueryInfo>();
        let rc = dpi::dpiStmt_getQueryInfo(stmt, pos, &mut qi);
        if rc != 0 {
            ctx.fail("DESCRIBE SELECT", rc);
        }
        names.push(std::slice::from_raw_parts(qi.name as *const u8, qi.name_length as usize).to_vec());
        let t = qi.type_info.oci_type_code;
        is_number.push(t == SQLT_NUM);
        let size = define_size(&qi, opt.longsize);
        row_bytes += (qi.type_info.client_size_in_bytes as usize).max(size as usize);
        let rc = dpi::dpiConn_newVar(
            conn,
            ORACLE_TYPE_VARCHAR,
            NATIVE_TYPE_BYTES,
            opt.fetch_array,
            size,
            1,
            0,
            std::ptr::null_mut(),
            &mut vars[pos as usize - 1],
            &mut datas[pos as usize - 1],
        );
        if rc != 0 {
            ctx.fail("DEFINE", rc);
        }
        let rc = dpi::dpiStmt_define(stmt, pos, vars[pos as usize - 1]);
        if rc != 0 {
            ctx.fail("DEFINE", rc);
        }
    }

    // output target
    let mut out: Box<dyn Write> = match &opt.outfile {
        Some(name) => {
            let path = String::from_utf8_lossy(name).into_owned();
            let r = if opt.append {
                std::fs::OpenOptions::new().create(true).append(true).open(&path)
            } else {
                File::create(&path).map(|f| f)
            };
            match r {
                Ok(f) => Box::new(f),
                Err(e) => {
                    eprintln!("chcsv: {path} Error: {e}");
                    std::process::exit(1);
                }
            }
        }
        None => Box::new(std::io::stdout()),
    };

    // header (the original encloses header cells unconditionally)
    if opt.header {
        let mut h = Vec::new();
        for i in 0..ncols {
            if opt.enclose != 0 {
                h.push(opt.enclose);
                h.extend_from_slice(&names[i]);
                h.push(opt.enclose);
            } else {
                h.extend_from_slice(&names[i]);
            }
            if i + 1 < ncols {
                h.push(opt.terminate);
            }
        }
        h.push(b'\n');
        if out.write_all(&h).is_err() {
            write_err();
        }
    }

    // Buffered CSV output; flushes every OUTRECORDS rows.
    // (The original has `if (BUFFERED = OUTRECORDS)` — an assignment — which
    // issues a write() per row; this port fixes it to `==`. Use -b 1 to get
    // the original per-row behavior.)
    let mut buf: Vec<u8> = Vec::with_capacity(row_bytes.saturating_mul(opt.out_records as usize).clamp(64 * 1024, 64 * 1024 * 1024));
    let mut total_rows: u64 = 0;

    loop {
        let mut row_index: u32 = 0;
        let mut nrows: u32 = 0;
        let mut more: std::os::raw::c_int = 0;
        let rc = dpi::dpiStmt_fetchRows(stmt, opt.fetch_array, &mut row_index, &mut nrows, &mut more);
        if rc != 0 {
            ctx.fail("FETCH", rc);
        }
        for h in 0..nrows {
            for i in 0..ncols {
                let d = &*datas[i].add((row_index + h) as usize);
                if opt.enclose != 0 && !is_number[i] {
                    buf.push(opt.enclose);
                }
                if d.is_null == 0 {
                    let b = &d.value.as_bytes;
                    // Safety: ODPI owns the buffer for this fetch cycle.
                    let s = std::slice::from_raw_parts(b.ptr as *const u8, b.length as usize);
                    buf.extend_from_slice(s);
                }
                if opt.enclose != 0 && !is_number[i] {
                    buf.push(opt.enclose);
                }
                if i + 1 < ncols {
                    buf.push(opt.terminate);
                }
            }
            buf.push(b'\n');
            total_rows += 1;
            if total_rows % opt.out_records as u64 == 0 {
                flush(&mut out, &mut buf);
            }
        }
        if nrows == 0 || more == 0 {
            break;
        }
    }

    if !buf.is_empty() {
        flush(&mut out, &mut buf);
    }

    for v in &vars {
        dpi::dpiVar_release(*v);
    }
    dpi::dpiStmt_release(stmt);
    dpi::dpiConn_commit(conn);
    dpi::dpiConn_release(conn);

    if total_rows == 0 && opt.nodata_exit {
        std::process::exit(100);
    }
    std::process::exit(0)
}

fn flush(out: &mut Box<dyn Write>, buf: &mut Vec<u8>) {
    if out.write_all(buf).is_err() {
        write_err();
    }
    buf.clear();
}

fn write_err() -> ! {
    eprintln!("chcsv: write Error");
    std::process::exit(1);
}

/// Buffer size for defining a column as text, mirroring the allocation rules
/// in SetDA()/sqlda.pc (with a small safety margin for conversion growth).
fn define_size(qi: &QueryInfo, longsize: u32) -> u32 {
    const MARGIN: u32 = 16;
    let t = qi.type_info.oci_type_code;
    match t {
        SQLT_NUM => {
            let prec = qi.type_info.precision;
            let scale = qi.type_info.scale;
            let mut numlen: i32 = prec as i32;
            if prec == 0 {
                numlen = 40; // Oracle supports 40 digits
            } else if scale < 0 {
                numlen -= scale as i32;
            }
            numlen as u32 + 2 + MARGIN
        }
        SQLT_LNG | SQLT_LNGRAW => longsize + 1 + MARGIN,
        SQLT_CLOB | SQLT_NCLOB | SQLT_BLOB => longsize.max(4000) + MARGIN,
        SQLT_DAT => 256 + MARGIN,
        SQLT_RDD => 19 + MARGIN,
        _ => {
            let mut l = qi.type_info.client_size_in_bytes.max(qi.type_info.size_in_chars);
            if l < 128 {
                l = 128;
            }
            l + 1 + MARGIN
        }
    }
}

unsafe fn bind_variables(ctx: &ContextHandle, conn: *mut Conn, stmt: *mut Stmt, opt: &mut Options) {
    let mut count: u32 = 0;
    let rc = dpi::dpiStmt_getBindCount(stmt, &mut count);
    if rc != 0 {
        ctx.fail("DESCRIBE BIND", rc);
    }
    if count == 0 {
        return;
    }
    let mut names: Vec<*const c_char> = vec![std::ptr::null(); count as usize];
    let mut lens: Vec<u32> = vec![0; count as usize];
    let mut n = count;
    let rc = dpi::dpiStmt_getBindNames(stmt, &mut n, names.as_mut_ptr(), lens.as_mut_ptr());
    if rc != 0 {
        ctx.fail("DESCRIBE BIND", rc);
    }
    for i in 0..n as usize {
        let name = std::slice::from_raw_parts(names[i] as *const u8, lens[i] as usize);
        let value: Vec<u8> = if i < opt.consts.len() {
            opt.consts[i].clone()
        } else if opt.infile.is_none() {
            eprintln!("chcsv: Not Specify combined variables");
            std::process::exit(1);
        } else {
            // interactive prompt, like GetValue() in sqlda.pc
            print!("Please set value {} = ", String::from_utf8_lossy(name));
            std::io::stdout().flush().unwrap();
            let mut v = Vec::new();
            std::io::stdin().lock().read_until(b'\n', &mut v).unwrap();
            if v.last() == Some(&b'\n') {
                v.pop();
            }
            println!("old: \"{}\" new:\"{}\"", String::from_utf8_lossy(name), String::from_utf8_lossy(&v));
            v
        };
        let mut var: *mut Var = std::ptr::null_mut();
        let mut data: *mut Data = std::ptr::null_mut();
        let rc = dpi::dpiConn_newVar(
            conn,
            ORACLE_TYPE_VARCHAR,
            NATIVE_TYPE_BYTES,
            1,
            value.len().max(1) as u32,
            1,
            0,
            std::ptr::null_mut(),
            &mut var,
            &mut data,
        );
        if rc != 0 {
            ctx.fail("BIND", rc);
        }
        let rc = dpi::dpiVar_setFromBytes(var, 0, value.as_ptr() as *const c_char, value.len() as u32);
        if rc != 0 {
            ctx.fail("BIND", rc);
        }
        let rc = dpi::dpiStmt_bindByName(stmt, name.as_ptr() as *const c_char, name.len() as u32, var);
        if rc != 0 {
            ctx.fail("BIND", rc);
        }
        dpi::dpiVar_release(var);
    }
}
