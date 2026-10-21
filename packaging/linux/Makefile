# Linux用のインストール。Debian・Arch・Nixのパッケージも、これを使う。
#
#   make -C packaging/linux install DESTDIR=/tmp/stage PREFIX=/usr
#
# 先に `cargo build --release -p pdf_view_app` でビルドしておく。
# 実行ファイルは `chihayaburu` という名前でインストールする(ビルド時の名前は pdf_view_app)。

PREFIX  ?= /usr/local
DESTDIR ?=
BINARY  ?= ../../target/release/pdf_view_app

# NAME: コマンド名とアイコン名。ID: デスクトップ環境がアプリを識別する名前(.desktopのファイル名、ウィンドウのapp_id)。
NAME    := chihayaburu
ID      := io.github.tora2rou.Chihayaburu
ICON_SIZES := 16 24 32 48 64 128 256

bindir   = $(DESTDIR)$(PREFIX)/bin
datadir  = $(DESTDIR)$(PREFIX)/share

.PHONY: install install-bin install-data uninstall check

install: install-bin install-data

install-bin:
	@test -f "$(BINARY)" || { echo "実行ファイルがありません: $(BINARY) (先に cargo build --release -p pdf_view_app)"; exit 1; }
	install -Dm755 "$(BINARY)" "$(bindir)/$(NAME)"

# 実行ファイル以外(デスクトップ環境に見せるもの)。Nixのように、実行ファイルを別の方法で
# 入れるパッケージ形式は、こちらだけを使う。
install-data:
	install -Dm644 $(ID).desktop "$(datadir)/applications/$(ID).desktop"
	install -Dm644 $(ID).metainfo.xml "$(datadir)/metainfo/$(ID).metainfo.xml"
	for size in $(ICON_SIZES); do \
		install -Dm644 icons/hicolor/$${size}x$${size}/apps/$(NAME).png \
			"$(datadir)/icons/hicolor/$${size}x$${size}/apps/$(NAME).png"; \
	done

uninstall:
	rm -f "$(bindir)/$(NAME)"
	rm -f "$(datadir)/applications/$(ID).desktop"
	rm -f "$(datadir)/metainfo/$(ID).metainfo.xml"
	for size in $(ICON_SIZES); do \
		rm -f "$(datadir)/icons/hicolor/$${size}x$${size}/apps/$(NAME).png"; \
	done

# .desktop と AppStream のメタ情報の検証(desktop-file-utils と appstream が要る)
check:
	desktop-file-validate $(ID).desktop
	appstreamcli validate --no-net $(ID).metainfo.xml
