# chcsv (Rust port)

Oracle Database に対して SQL を実行し、SELECT の結果を CSV で出力する CLI です。1995 年に公開された Pro*C ベースの [chcsv](https://github.com/moriyasujapan/chcsv)（作者: Batayan、GPL）の Rust ポートです。

English: run SQL against Oracle and dump SELECT results as CSV. A Rust port of the 1995 Pro*C chcsv. No Pro*C needed — `cargo build` is enough.

```console
$ echo "select id, name from emp" | ./chcsv scott/tiger@dbhost:1521/ORCL -h -e '"'
"ID","NAME"
7369,"SMITH"
7499,"ALLEN"
```

## なぜ移植したのか

原版のビルドには Oracle Pro*C プリコンパイラ（`$ORACLE_HOME/precomp`）が必要です。本ポートは cargo と gcc だけでビルドでき、ODPI-C を静的にリンクし、Oracle クライアントライブラリは実行時に dlopen します。

性能についての計測結論を先に書きます。Rust 化そのものの高速化効果はほぼゼロでした。原版も全列を文字列（SQLT_STR）で define するため型→ASCII 変換は Oracle サーバー側で行われ、クライアントの熱い経路は memcpy と write だけです。200,000 行の実測でクライアントの CPU 使用は全体の約半分（0.3 秒 / 0.7 秒）にすぎず、残りは DB 応答とネットワーク待ちです。

実測でわかった本当の問題は原版のバッファリングバグです。`OutPut.pc:82` の `if( BUFFERED = OUTRECORDS )` は比較（`==`）ではなく代入なので条件が常に真になり、1 行ごとに `write()` が走ります。本ポートはこれを修正し、OUTRECORDS 行ごとのバッチ書き込みにしました（strace 実測: write syscall が 202,012 回 → 2,052 回）。原版の 1 行ごと書き込みを再現したい場合は `-b 1` を使ってください。

## セットアップ

必要なもの:

- Linux x86-64（ほかのプラットフォームでは build.rs の rpath 設定の調整が必要）
- rust（edition 2021 対応）、gcc、git、curl、unzip
- Oracle Instant Client（プロプライエタリなライセンスのため本リポジトリに含まれません。各自でダウンロードしてください）
- libaio。Ubuntu 24.04 は `libaio.so.1t64` のみを提供します。`setup.sh` が symlink を作ります

手順:

```console
$ git clone https://github.com/moriyasujapan/chcsv-rust.git
$ cd chcsv-rust
$ ./setup.sh                 # ODPI-C を vendor/odpi に取得
$ unzip instantclient-basiclite-linux.x64-21.zip .   # 任意の場所でも可
$ cargo build --release
```

Oracle Instant Client は Oracle のダウンロードページ（[Linux x86-64](https://www.oracle.com/database/technologies/instant-client/linux-x86-64-downloads.html)）から basic もしくは basic-lite ZIP を取得します。21.x か 19.x を推奨します。

実行時には Instant Client のディレクトリをローダに教えます（libclntsh 自体に RPATH がないため、これは Oracle 公式手順と同じ制約です）。

```console
$ export LD_LIBRARY_PATH=$PWD/instantclient_21_21
$ export NLS_LANG=AMERICAN_AMERICA.AL32UTF8   # DB の文字セットに 맞춰ください
$ ./target/release/chcsv user/pass@host:1521/SERVICE -h < query.sql
```

`setup.sh --with-instant-client <ZIP URL>` を使うとダウンロードと解凍も一括でできます。

### 预ビルドバイナリ（Linux / Windows / macOS）

[Releases](https://github.com/moriyasujapan/chcsv-rust/releases) に各 OS 用の
バイナリを置いています（GitHub Actions のネイティブ runner でビルド）。
Oracle Instant Client は実行時に `dlopen` / `LoadLibrary` で読み込むため
リンクしておらず、配布物に同梱していません。Instant Client（basic または
basic-lite ZIP）は別途 Oracle から入手してください。

| アセット | 動作環境 | 実行時に必要なもの |
|---|---|---|
| `chcsv-linux-x86_64` | glibc 2.28 以上の Linux（RHEL/OL 8・9・10、Ubuntu・Debian 等。rockylinux:8 でビルド） | `dnf install libaio libnsl` + Linux 用 Instant Client（`LD_LIBRARY_PATH` に指定） |
| `chcsv-windows-x86_64.exe` | Windows x64 | [Windows 用 Instant Client](https://www.oracle.com/database/technologies/instant-client/winx64-64-downloads.html)（`oci.dll` を PATH か exe と同じフォルダに） |
| `chcsv-macos-x86_64` | Intel Mac（Apple Silicon では Rosetta 経由でも可） | [macOS Intel 用 Instant Client](https://www.oracle.com/database/technologies/instant-client/macos-intel-x86-downloads.html) |
| `chcsv-macos-aarch64` | Apple Silicon（M1 以降）ネイティブ | [macOS ARM64 用 Instant Client](https://www.oracle.com/database/technologies/instant-client/macos-arm64-downloads.html) |

```console
$ sudo dnf install libaio libnsl
$ export LD_LIBRARY_PATH=/path/to/instantclient_21_21
$ ./chcsv-linux-x86_64 user/pass@host:1521/SERVICE -h < query.sql
```

macOS では `export DYLD_LIBRARY_PATH=/path/to/instantclient_XX` を、Windows では
Instant Client フォルダを PATH に追加するか `oci.dll` を exe と同じ場所において
から実行します。

### Docker / docker compose で使う

Instant Client を同梱したイメージをビルドする `docker/Dockerfile` と、
Oracle XE を立ててそのまま試せる `compose.yaml` を同梱しています。

```console
# イメージを作る（rockylinux:8 でビルド → OL8 + Instant Client のランタイム）
$ docker build -f docker/Dockerfile -t chcsv .

# 既存DBへ接続だけする（Oracle は起動しない）
$ echo "select id, name from emp" | docker compose run --rm -T chcsv user/pass@host:1521/SERVICE -h > out.csv

# Oracle XE 21 込みで丸ごと試す（demo ユーザ自動作成、host側は 1522 ポート）
$ docker compose --profile demo up -d oracle      # 起動に1〜2分
$ echo "select 3 as n from dual" | docker compose --profile demo run --rm -T chcsv demo/demo123@oracle:1521/XEPDB1 -h
N
3
$ docker compose --profile demo down
```

`docker/Dockerfile` はビルド時に Oracle 公式 URL から Instant Client ZIP を
取得します（リポジトリに Oracle バイナリは含みません）。Instant Client は
[OTN Development and Distribution License Terms](https://www.oracle.com/downloads/licenses/instant-client-lic.html)
の下、アプリと束ねる形での無償再配布が許可されています。ビルド済みイメージを
再配布する場合は同条項に従ってください。

## 使い方（原版互換のリファレンス）

```
chcsv userid/passwd[@connect] [オプション...] [const] [const] ...
```

SQL は標準入力、または `-i` でファイルを指定します。SQL 内の `&name` / `:name` がバインド変数です。定数（positional const）を左から順にバインドします。定数が足りないとき、`-i` 併用ならプロンプト（`Please set value NAME = `）で対話入力を受け付け、それ以外ならエラー終了します。

| オプション | 既定値 | 意味 |
|---|---|---|
| `-o FILE` | 標準出力 | 出力ファイル |
| `-a FILE` | — | 出力ファイルへ追記 |
| `-i FILE` | 標準入力 | SQL をファイルから読む（対話バインド有効化） |
| `-e C` | なし | 各欄を文字 C で囲む（NUMBER 欄は囲まない） |
| `-t C` | `,` | 区切り文字 |
| `-l N` | 1000 | LONG 系の取得長 |
| `-f N` | 100 | フェッチ配列サイズ |
| `-b N` | 100 | 出力バッファのフラッシュ間隔（行） |
| `-v` | — | 縦形式（区切りを改行に変更） |
| `-h` | — | 1 行目にヘッダ（列名）を出力 |
| `-n` | — | 0 行なら exit 100 |

注意点（原版の仕様どおり）:

- オプション値はフラッドに貼り付け（`-f100`）か、次の argv トークンで渡します。次のトークンが `-` で始まっても無検証で使います
- `-a` は原版で `-o` にフォールスルーします。`-a FILE` は FILE への追記です
- 数値の表現（例: `-0.75` → `-.75`）は Oracle の型→ASCII 変換の結果で、原版・SQL\*Plus と一致します
- 終了コード: 正常 0、0 行 + `-n` で 100、Oracle エラーは `ORA-xxxxx` の下 8 ビット（例: ORA-00942 → 249）

使用例:

```console
$ echo "select * from demo where id > &MIN" | ./chcsv scott/tiger@db -h -e '"' -b 5000 100
$ printf 'select name from emp where deptno=&D\n' | ./chcsv scott/tiger@db -i q.sql 10
$ echo "update emp set sal=sal*1.1" | ./chcsv scott/tiger@db   # SELECT 以外は黙って実行し commit
```

## 原版との互換性と改善点

検証済みで一致（Oracle XE 21 + AL32UTF8 で実測）: 区切り・囲み・ヘッダ・縦形式の末尾空行・NULL を空欄・NUMBER を囲まない・バインド（定数/対話/不足時エラー）・`-o`/`-a`・DML サイレント実行と commit・`-n` と ORA の終了コード・エラーメッセージ書式（`chcsv: <処理> Error, Code=...`）。

改善点（意図的な差異）:

- **バッチ書き込み**: 上記のバグ修正。`-b 1` で原版挙動を再現できます
- **CLOB / TIMESTAMP 対応**: 原版はバッファ不足で正しく出力できませんでした。`-l` の長さと関係なく 4000 文字まで取得します
- SQL 末尾の `;` と後続空白を除去してから prepare します
- 対話入力の末尾改行を除去します（原版は改行ごとバインドしていました）

既知の差異: SIGINT 等のシグナル処理は原版のような後始末ログなしで即終了します。

## 実測ベンチマーク

環境: Oracle XE 21（Docker、同一ホスト）、BENCH 表 200,000 行。原版 C バイナリは
OL8 コンテナ内で Instant Client + Pro*C 21 によりビルドしたもの（手順は
[BUILD-ORIGINAL-C-OL8.md](BUILD-ORIGINAL-C-OL8.md)）。CSV 出力は全構成で
**md5 一致**（10.7MB、バイト単位同一）を確認。

**原版 C バイナリとの直接比較**（200k 行 → 実ディスクへ書き出し、4 構成交互 7 回の平均）:

| 構成 | 平均実時間 | vs 原版 |
|---|---|---|
| 原版 C（1 行毎 write のバグ入り） | 1.011 s | — |
| 原版 C を `==` に修正 | 0.823 s | −18.6% |
| 本移植 `-b 1`（原版バグ再現） | 0.924 s | −8.6% |
| **本移植 既定（`-b 100`）** | **0.772 s** | **−23.7%** |

バグ修正済みの C に対しても約 6% 速く、全ラウンドで一貫しました（Jev 有意性 3.8/5）。
`-o /dev/null` で測ると write が無料になるため順位が揺れます。実ファイル出力で
測ってください。

参考（本移植のみ、`-o /dev/null`、ローカルでは DB 待ちが支配的）:

| 構成 | 実時間 | write() syscall |
|---|---|---|
| 既定（`-f 100 -b 100`） | 0.62〜0.72 秒 | 2,052 |
| 原版バグ再現（`-b 1`） | 0.78〜0.88 秒 | 202,012 |
| チューニング（`-f 1000 -b 5000`） | 0.67〜0.71 秒 | 2,052 |

ローカル接続では DB 待ちが支配的なので実時間差は穏やかです。リモート DB では 100 分の 1 の syscall 数の差がそのまま効きます。大量データでは `-f`/`-b` を上げてください。

テスト用 DB は `docker run -d -p 127.0.0.1:1521:1521 -e ORACLE_PASSWORD=<pw> gvenzl/oracle-xe:21-slim` で用意できます。

## ディレクトリ構成とライセンス

```
build.rs        ODPI-C をコンパイルし ODPI のバージョン情報を注入
src/dpi.rs      ODPI-C の FFI 手書きバインディング
src/main.rs     CLI・CSV 出力本体
setup.sh        ODPI-C の取得と Instant Client の案内
```

- 本ポート: GPL-2.0-or-later（原版 chcsv のライセンスを継承。Copyright (C) 1995 Batayan）
- `setup.sh` が取得する ODPI-C: MIT / Apache-2.0（`vendor/odpi/`、本リポジトリには含まれません）
- Oracle Instant Client: Oracle のプロプライエタリライセンス（本リポジトリには含まれません）
