# 原版 chcsv（Pro*C / 1995）を Oracle Linux 8 で現代ビルドする recipe

本リポジトリの Rust 移植とは**独立**の参考ドキュメントです。原版 chcsv の
ソース（`*.pc` / `*.h`、Oracle 純正物ではないが本リポジトリには同梱しない）を、
Oracle Instant Client と Pro*C プリコンパイラだけで OL8 上にビルドする手順を、
2025 年に実測で通ったまま記録します。完全な互換確認（出力バイト一致）は
Rust 移植側の README を参照してください。

前提: `oraclelinux:8` コンテナ（`--network host`、ソースを `/root/src` に配置）、
gcc / make / unzip / cpp 済み。

## 1. 部材（認証不要な ZIP だけで揃う）

```console
# Instant Client 21.21 basic-lite + SDK（ランタイムとヘッダ）
curl -O https://download.oracle.com/otn_software/linux/instantclient/2121000/instantclient-basiclite-linux.x64-21.21.0.0.0dbru.zip
curl -O https://download.oracle.com/otn_software/linux/instantclient/2121000/instantclient-sdk-linux.x64-21.21.0.0.0dbru.zip
# Pro*C プリコンパイラ 21.24（proc 本体と sqlca.h/sqlda.h 入り SDK）
curl -O https://download.oracle.com/otn_software/linux/instantclient/2124000/instantclient-precomp-linux.x64-21.24.0.0.0dbru.zip
# これらを /opt/ic/instantclient_21_21 と /opt/ic/instantclient_21_24 に解凍
```

## 2. 判明した 6 つの落とし穴と対処

1. **`dynamic=SQLDA` は proc 21 で廃止**（PCC-F-02044）。原版 BUILD の
   プリコンパイラオプションから削除する（chcsv の SQLDA は手製なので不要）。
2. **proc は ORACLE_HOME 風の `precomp/admin/pcscfg.cfg` を要求**（PCC-F-02066、
   LD_LIBRARY_PATH から逆算）。basic-lite には無いので手作成:
   ```console
   mkdir -p /opt/ic/instantclient_21_21/precomp/admin
   printf "sys_include=/root/psafe\n" > /opt/ic/instantclient_21_21/precomp/admin/pcscfg.cfg
   ```
   ※ `codegen=` 等の旧オプションは pcc 系と異なり不可（PCC-F-02041）。
3. **proc 現行パーサは現代 glibc ヘッダを解釈できない**（`size_t` 等で
   PCC-S-02201 連発）。C 側システムヘッダを**空スタブ**で回避する。
   proc 通过りのみスタブ、gcc は実ヘッダを見るので安全:
   ```console
   mkdir /root/psafe
   for h in stdio.h stdlib.h string.h strings.h errno.h fcntl.h unistd.h time.h \
            signal.h stdarg.h stddef.h setjmp.h locale.h ctype.h; do : > /root/psafe/$h; done
   # FILE 型だけ最小限必要:
   cat > /root/psafe/stdio.h <<'EOF'
   typedef struct _pFILE FILE;
   extern FILE *stdin, *stdout, *stderr;
   extern int printf(), fprintf(), sprintf(), fputs(), fputc(), fclose(), fscanf();
   extern FILE *fopen(), *freopen();
   extern int remove(), rename();
   EOF
   cp /opt/ic/instantclient_21_24/sdk/include/sqlca.h /opt/ic/instantclient_21_24/sdk/include/sqlda.h /root/psafe/
   ```
4. **`parse=PARTIAL` は使えない**: PARTIAL は `EXEC SQL INCLUDE` を翻訳せず、
   かといって `#include` に置換するとヘッダ内の `EXEC SQL TYPE`（VARCHAR）が
   PCC-F-02397 で弾かれる。**FULL 一筋** + 上記スタブで回避する。
5. **proc 21 は EXEC SQL の host variable に DECLARE SECTION 内宣言を要求**
   （PCC-S-02322/02330）。1995 年はグローバル変数で通っていた。chcsv は
   `sqlda.pc` の `EXEC SQL FOR :FETCH_ARRAY FETCH` だけが引っかかるため、
   DECLARE SECTION 部に `extern int FETCH_ARRAY;` を 1 行追加する。
6. **`sys_errlist` の const 衝突**: 1995 の `extern char *sys_errlist[];` は
   現行 glibc の `const char *const sys_errlist[]` と衝突する。生成 C を
   `extern const char *const sys_errlist[];` に置換（`-w` では消えない実エラー）。

## 3. ビルド

```console
export LD_LIBRARY_PATH=/opt/ic/instantclient_21_21
P=/opt/ic/instantclient_21_24/sdk/proc
cd /root/src   # chcsv.pc Option.pc OutPut.pc Signal.pc sqlda.pc + *.h

# プリコンパイル（code=ANSI_C。sqlca の実体は 1 箇所だけ必要）
for f in chcsv Option OutPut Signal sqlda; do
  $P code=ANSI_C "sys_include=/root/psafe" "include=/root/src" $f.pc oname=$f.c
done
printf '#include <sqlca.h>\nstruct sqlca sqlca;\n' > sqlca_inst.c

# コンパイル / リンク（oratypes.h は 21_21 側、sqlcpr.h は 21_24 側にしかない点に注意。
# libclntshcore の明示リンクと rpath-link も必須）
gcc -w -I/root/src -I/opt/ic/instantclient_21_21/sdk/include \
    -I/opt/ic/instantclient_21_24/sdk/include \
    -c chcsv.c Option.c OutPut.c Signal.c sqlda.c sqlca_inst.c
gcc -o chcsv chcsv.o Option.o OutPut.o Signal.o sqlda.o sqlca_inst.o \
    -L/opt/ic/instantclient_21_21 -lclntsh -lclntshcore \
    -Wl,-rpath-link,/opt/ic/instantclient_21_21
```

## 4. 既知のバグ（原版動作を再現する場合）

`OutPut.pc:82` が `if( BUFFERED = OUTRECORDS )`（`==` の誤り）で、`-b`
指定に関係なく 1 レコードごとに write() する。実証（200k 行・実ディスク書き出し）:

| 構成 | 平均（7 回交互） |
|---|---|
| 原版（バグ入り） | 1.011 s |
| `==` に修正 | 0.823 s |
| Rust 移植（既定 `-b 100`） | 0.772 s |

Rust 移植は既定でこのバグを修正しており、出力は原版とバイト単位一致
（DEMO・BENCH 双方で md5 一致を確認）。
