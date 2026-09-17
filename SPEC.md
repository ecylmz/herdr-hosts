# Herdr Hosts Plugin

Herdr 0.9.1 için hafif, hızlı ve klavye odaklı bir SSH host launcher'ı.

Amaç Remote Desktop Manager benzeri ağır bir uygulama yapmak değildir. Herdr
zaten terminal/pane/tab yönetimini sağlıyor. Bu eklentinin görevi yalnızca çok
sayıdaki SSH hostunu düzenlemek, bulmak ve seçilen hosta Herdr terminal pane'i
üzerinden bağlanmayı kolaylaştırmaktır.

Tek akış:

```text
tuş → picker açılır → host bul → Enter → ssh pane'i açılır
```

> Bu doküman Herdr 0.9.1 binary'si, `herdr api schema --json` çıktısı ve
> resmi plugin dokümantasyonu üzerinden doğrulanmıştır. §3 doğrulanmış
> gerçekleri listeler; implementasyon sırasında bunları yeniden araştırma.
>
> **Durum: v0.1 tamam.** §19'daki Faz 1-5 gerçeklendi ve çalışıyor.
> Kalan tek kapsam v0.2 sidebar (§21).

---

# 1. Temel hedef

Plugin, Herdr'ın **popup** placement'ı ile açılan bir Ratatui TUI'sidir.

Kullanıcı bir keybinding'e basar, ekranın ortasında modal bir picker açılır ve
SSH hostlarını hiyerarşik klasörler altında gösterir:

```text
╔════════════════════════════════════╗
║ SSH HOSTS                  /prod   ║
║                                    ║
║ ★ Favorites                        ║
║   ● dokku-prod                     ║
║                                    ║
║ ▾ Personal                         ║
║   ▾ Hetzner                        ║
║     ● dokku-prod                   ║
║     ● postgres                     ║
║ ▾ University                       ║
║   ▾ Production                     ║
║     ● web-prod                     ║
╚════════════════════════════════════╝
```

Host üzerinde Enter → Herdr içinde yeni bir terminal pane açılır, içinde
`ssh <alias>` çalışır, popup kendiliğinden kapanır.

SSH bağlantısı plugin tarafından implemente edilmez. OpenSSH kullanılır.

## Neden popup, neden sidebar değil

Sol dock'lu kalıcı sidebar üç pahalı problemi birden getiriyor: Herdr'da split
yönü yalnızca `right`/`down` olduğu için sol dock split+swap gerektiriyor;
genişlik ratio tabanlı olduğu için "sabit 30 sütun" diye bir şey yok; ve normal
bir pane olduğu için her yeni tab/workspace'te event hook'larla yeniden
yaratılması gerekiyor. Referans implementasyonun (`alexarthurs/herdr-sidebar`)
kodunun büyük kısmı bu üç iş için.

Popup bunların üçünü de ortadan kaldırır: layout'a dokunmaz, pane ID'si yoktur,
komut bitince kendini kapatır. §25'in testine ("kullanıcıyı hosta daha hızlı
götürüyor mu?") popup daha net cevap veriyor.

Kalıcı sidebar v0.2 kapsamındadır (§21). TUI/parser/search kodunun neredeyse
tamamı ortaktır; yalnızca "nasıl açıldığı" değişir. Bu yüzden v0.1'de docking'e
bağımlı hiçbir varsayım yapma.

---

# 2. Mimari sınırlar

## Herdr core değiştirilmemeli

Herdr fork'lanmamalı. Native Herdr sidebar'a `HOSTS` bölümü eklemeye çalışma.
Yalnızca Herdr 0.9.1 plugin manifest'i ve CLI/socket API'si kullanılmalı.
Undocumented davranışa yaslanma.

## Referans implementasyon

`https://github.com/alexarthurs/herdr-sidebar` — MIT lisanslı, Rust + ratatui,
aktif. Plugin manifest yapısı, `plugin.pane.open` kullanımı ve pane ID
çıkarımı için iyi bir referans.

Kod kopyalarsan MIT copyright ve lisans metnini taşı. Yaklaşımı incelemek ve
yeniden yazmak serbest. Körlemesine kopyalama — o proje sidebar docking ve
Windows desteği için bizde olmayacak karmaşıklığı taşıyor.

---

# 3. Doğrulanmış Herdr 0.9.1 gerçekleri

Bunlar test edilmiştir, varsayım değildir.

## 3.1 Plugin sistemi

Manifest `herdr-plugin.toml`, plugin kökünde. Zorunlu alanlar: `id`, `name`,
`version`, `min_herdr_version`. Bölümler: `[[build]]`, `[[startup]]`,
`[[actions]]`, `[[events]]`, `[[panes]]`, `[[link_handlers]]`.

Kurulum:

```bash
herdr plugin link /path/to/plugin     # lokal geliştirme (build adımları çalışmaz)
herdr plugin install owner/repo       # GitHub'dan (build adımları çalışır)
```

Runtime'da enjekte edilen env değişkenleri:

```text
HERDR_SOCKET_PATH  HERDR_BIN_PATH  HERDR_ENV=1
HERDR_PLUGIN_ID    HERDR_PLUGIN_ROOT
HERDR_PLUGIN_CONFIG_DIR    HERDR_PLUGIN_STATE_DIR
HERDR_PLUGIN_CONTEXT_JSON
HERDR_WORKSPACE_ID  HERDR_TAB_ID  HERDR_PANE_ID   (mevcutsa)
HERDR_PLUGIN_ENTRYPOINT_ID                        (pane komutlarına)
```

Komutlar plugin dizini cwd olarak çalışır. `HERDR_BIN_PATH` ile `herdr`
CLI'ını çağır; ham socket transport'u OS'a göre değişiyor, CLI değişmiyor.

## 3.2 Popup davranışı

`placement = "popup"`:

* Session-modal, tiled layout'u değiştirmez.
* `width` / `height` manifest'te veya open isteğinde; sayı = terminal hücresi,
  `"80%"` = yüzde. Minimumun altı clamp edilir.
* **Escape dahil tüm klavye girdisini alır.**
* **Komut çıkınca kapanır** (veya `popup.close` isteğiyle).
* Pane ID'si yoktur, `HERDR_PANE_ID` almaz, pane lifecycle event'i yaymaz,
  layout/persistence/agent API'lerine katılmaz.
* Altındaki tiled pane `HERDR_PLUGIN_CONTEXT_JSON` içinden okunur.
* Settings/Copy mode/başka bir modal açıkken `ui_busy` döner — bu hata
  kullanıcıya anlaşılır gösterilmeli.

## 3.3 Komut çalıştırma — kritik kısıt

**`pane.split` şemasında `command` alanı YOKTUR.** Parametreleri yalnızca:
`direction`, `cwd`, `env`, `ratio`, `target_pane_id`, `workspace_id`, `focus`,
`right_click`.

API'nin 111 metodunun **hiçbiri** bir pane'de keyfi argv çalıştırmaz.
`herdr pane run` shell'e metin yazar — yani tanımı gereği shell interpolation'dır.
`pane.send_text` / `pane.send_keys` de aynı şekilde.

Bu yüzden `pane split` + `pane run` **kullanılmayacak**. Bkz. §9.

## 3.4 Kullanılacak API yüzeyi

```text
plugin.pane.open    plugin_id, entrypoint, placement, direction,
                    target_pane_id, cwd, env, focus, width, height
                    → .result.plugin_pane.pane.pane_id

pane.list           workspace_id (opsiyonel) → pane listesi
pane.focus          pane_id ile doğrudan focus (yön değil)
pane.report_metadata pane_id, source, title, tokens (max 16,
                    ^[A-Za-z0-9_-]{1,32}$), state_labels, ttl_ms
popup.close
```

CLI karşılıkları: `herdr plugin pane open|focus|close`, `herdr pane list|get`.
Çoğu komut JSON döner; ID'leri tahmin etme, cevaptan oku.

Pane ID formatı `w1:p1` — workspace-qualified, opaque, yeniden kullanılmaz.

## 3.5 Keybinding

Herdr `config.toml` içinde plugin action'a tuş bağlanabilir:

```toml
[[keys.command]]
key = "prefix+h"
type = "plugin_action"
command = "herdr-hosts.open"
description = "SSH hosts"
```

Bu, §18'in "hack yapma" şartını karşılar — resmi mekanizma.

## 3.6 Çakışma kontrolü

`herdr machine` komutu "saved SSH machines" yönetir, ancak bu **uzak makinede
Herdr server'ı çalıştırmak** içindir (`herdr --machine <label> pane list`).
Düz `ssh` oturumu açmaz. Bu plugin ile işlevsel çakışması yoktur.

---

# 4. Teknoloji

```text
Rust        ratatui      TUI + crossterm
            serde_json   Herdr CLI cevapları
            glob         Include genişletme
```

TOML crate'i yok: tek yazılan dosya satır başına bir alias içeren favorites
listesi, ve tek okunan dosya `~/.ssh/config`.

Tek crate, iki `[[bin]]`: TUI ve ssh launcher (§9). Aynı release artifact'ı.

Linux öncelikli. macOS mimari olarak bozulmamalı; v0.1 için Linux yeterli.
Windows kapsam dışı.

Electron, webview, browser UI, embedded web server yok.

---

# 5. Veri modeli

**Tek kaynak: `~/.ssh/config`.** Ayrı bir inventory dosyası yoktur.

Gerekçe: inventory.toml denendi ve kaldırıldı. İki dosya, yeni host eklerken
iki yere yazmak demekti — ssh config'e host, inventory'ye klasör kaydı. Oysa
host zaten doğru başlığın altına yazıldığında gruplama bedava geliyor.

## Klasörler — yorum başlıkları

```sshconfig
# --- Personal / Hetzner ---
Host dokku-prod   # main dokku box
Host postgres

# === University ===
Host web-prod
```

Başlık kuralı: `#` sonrası metin, aynı ayraçtan (`-`, `=`, `*`) en az ikisiyle
hem başlamalı hem bitmeli. `/` iç içe klasör açar.

Kural bilinçli olarak dar: `# şu anahtarı yenile` gibi sıradan bir yorum
başlık sayılmaz, yani mevcut config'lerin anlamı bozulmaz. Başlıksız bir
config tek düz liste (`Ungrouped`) olarak görünür — doğru varsayılan.

## Not — satır sonu yorumu

```sshconfig
Host dokku-prod   # main dokku box
```

**Ölçüldü:** OpenSSH `Host` satırında `#` sonrasını yok sayıyor (`ssh -G` ile
doğrulandı), dolayısıyla not yazmanın maliyeti sıfır.

## Favorites — plugin'in yazdığı tek şey

`$HERDR_PLUGIN_STATE_DIR/favorites`, satır başına bir alias.

`~/.ssh/config`'e **yazılmaz**. O dosya kullanıcının bütün makinelerine erişim
yolu; bozulursa ssh'ın tamamı gider ve plugin'in orada güvenli in-place yazma
garantisi yok. Herdr de "runtime state buraya" diyor (§3.1).

Yazma atomic (temp + rename).

## Kapsam dışı

OpenSSH'ın grup kavramı yok — `ssh_config(5)`'te "group" yalnızca şifre
takımı adlarında geçiyor. Bu konvansiyon plugin'e ait; `ssh`'ın dosyayı nasıl
okuduğunu hiçbir şekilde değiştirmez.

# 6. SSH config parser

`~/.ssh/config` içindeki gerçek host alias'larını okuyabilmeli.

SSH config'i yeniden implemente etmeye çalışma. v0.1 için gereken tek şey
**alias listesi**:

* `Host` satırlarını topla, whitespace ile ayrılmış birden fazla alias olabilir.
* Wildcard/negation içerenleri (`*`, `?`, `!`) host olarak gösterme.
* `Include` direktifini işle: glob genişlet, tekrarlı include'lara karşı
  ziyaret edilen dosyaları takip et.
* **Göreli `Include` daima `~/.ssh` altında çözülür** — include *eden* dosyanın
  dizininde değil. Ölçüldü: `~/.ssh/config.d/10-work` içindeki `Include common`
  OpenSSH'ta `~/.ssh/common`'ı okuyor. Bu kolayca yanlış yazılır; testte
  sabitlenmiştir.
* **`#` yalnızca token sınırında yorum başlatır.** Ölçüldü: `Host beta#gamma`
  OpenSSH için tek alias (`ssh -G beta` eşleşmiyor). Koşulsuz `find('#')`
  kullanılırsa picker `beta` gösterip `ssh beta` çalıştırır — yani gösterilen
  host ile bağlanılan host farklı olur.
* Case-insensitive anahtar karşılaştır (OpenSSH öyle davranıyor).

`HostName`/`User`/`Port`/`IdentityFile`/`ProxyJump` değerleri v0.1 UI'ında
gösterilmiyor, dolayısıyla parse edilmeleri gerekmiyor. Gerçekten lazım
olurlarsa OpenSSH'in kendisine sor:

```bash
ssh -G <alias>
```

Bu Include, Match ve wildcard'ı doğru çözer; kendi parser'ın çözemez.

Crate değerlendirmesi: mevcut Rust crate'lerinin Include desteği zayıf. Yukarıdaki
kapsam ~60 satır; kendi kontrollü tarayıcını yaz.

---

# 7. Ana UI

```text
╔════════════════════════════════════╗
║ SSH HOSTS                          ║
║                                    ║
║ ★ Favorites                        ║
║   ● dokku-prod                     ║
║                                    ║
║ ▾ Personal                         ║
║   ▾ Hetzner                        ║
║     ● dokku-prod                   ║
║     ● postgres                     ║
║     ● backup                       ║
║                                    ║
║ ▾ University                       ║
║   ▾ Production                     ║
║     ● web-prod                     ║
╟────────────────────────────────────╢
║ / search            3 hosts    [?] ║
╚════════════════════════════════════╝
```

Popup boyutu: `width = "60%"`, `height = "70%"` civarı. Manifest'te tanımlı,
open isteğinde override edilmez.

UI temiz ve minimal. Aşırı ASCII decoration yok. **TUI kendi border/başlığını
çizmez** — popup çerçevesini ve entrypoint başlığını Herdr zaten çiziyor,
tekrarı hem başlığı iki kez gösterir hem iki satır yer yer.

Terminal renklerini mevcut theme'den al. **Hardcoded renk şeması kullanma** —
seçili satır için reversed/bold, ikincil metin için `Color::DarkGray` gibi
ANSI-relative değerler yeterli. RGB hex verme.

Alt satır aynı zamanda status line'dır (§20).

---

# 8. Klavye

```text
↑ / k        önceki
↓ / j        sonraki
→ / l        klasörü aç ve içine gir
← / h        açık klasörü kapat, kapalıysa üst klasöre çık
Enter        host → bağlan;  klasör → aç/kapa (imleç klasörde kalır)
Space        klasör aç/kapa
/            fuzzy search
Esc          search açıksa kapat, değilse popup'ı kapat
f            favorite toggle
r            reload (ssh config + favorites)
?            help
q            çık
```

Yatay gezinme dosya yöneticisi davranışını izler: `→` açıp içeri iner, `←`
önce kapatır, sonra bir üst seviyeye çıkar.

Popup Escape'i yakaladığı için Esc'in iki kademeli davranışı önemli:
search modundayken yanlışlıkla tüm popup'ı kapatmamalı.

---

# 9. SSH bağlantısının açılması

En önemli bölüm budur. §3.3 nedeniyle `pane split` + `pane run` kullanılmaz.

İki adımlı çözüm:

**1. Manifest'te ayrı bir pane entrypoint:**

```toml
[[panes]]
id = "ssh"
title = "SSH"
placement = "tab"
command = ["./target/release/herdr-hosts-ssh"]
platforms = ["linux", "macos"]
```

**2. TUI, alias'ı env ile geçirerek bu entrypoint'i açar:**

```bash
herdr plugin pane open \
  --plugin herdr-hosts --entrypoint ssh \
  --placement tab \
  --env HERDR_HOSTS_ALIAS=dokku-prod \
  --focus
```

Ardından sekme adlandırılır ve açıkça focus edilir:

```bash
herdr tab rename <tab_id> dokku-prod
herdr tab focus  <tab_id>
```

`tab_id` open cevabında `.result.plugin_pane.pane.tab_id` olarak gelir.

`--focus` yeni pane'i zaten focus ediyor ve ölçüldü: popup kapanışı bunu geri
almıyor. Yine de `tab focus` açıkça çağrılır — Enter'a basmanın tek amacı o
oturuma varmak, ve picker aynı anda kapanan modal bir popup. Aynı çağrı
mevcut oturuma dönerken de yapılır (`focus_pane`).

`herdr-hosts-ssh` binary'si `HERDR_HOSTS_ALIAS`'ı okur ve:

```rust
Command::new("ssh").arg(&alias).status()
```

`exec` denendi ve geri alındı: ssh bağlantı hatasıyla çıkınca pane anında
kapanıyor ve kullanıcı `Connection refused` mesajını göremiyor. Launcher ssh'ı
child olarak çalıştırır; ssh'ın kendi hata kodu (255) döndüğünde mesajı basıp
Enter bekler, normal çıkışta pane hemen kapanır.

Sonuç: Herdr launcher'ı entrypoint'i argv ile spawn eder, launcher `ssh`'ı argv
ile exec eder. **Hiçbir noktada shell yoktur**, dolayısıyla interpolation da
yoktur. §8'in orijinal şartı bu yolla karşılanır.

Buna rağmen alias'ı trust boundary'de doğrula: `^[A-Za-z0-9._-]+$` dışındaki
her şeyi reddet, `-` ile başlayanı reddet (ssh flag'i olarak yorumlanmasın).
Alias `~/.ssh/config`'ten geliyor ama o dosya da kullanıcı girdisidir.

Pane açıldıktan sonra TUI `exit(0)` yapar; popup kendiliğinden kapanır.

`plugin.pane.open` `ui_busy` veya hata dönerse popup kapanmaz, status line'da
hata gösterilir.

---

# 10. Aynı host zaten açıksa

Pane açtıktan sonra etiketle:

```bash
herdr pane report-metadata <pane_id> \
  --source herdr-hosts --title "ssh:dokku-prod"
```

`tokens` alanı da kullanılabilir (max 16 anahtar, `^[A-Za-z0-9_-]{1,32}$`).
Alias token değerine sığmayabilir; title güvenilir olan.

Enter'a basıldığında önce `herdr pane list` ile mevcut pane'leri tara, eşleşen
başlık varsa yeni sekme açmak yerine:

```bash
herdr pane focus <pane_id>     # socket: pane.focus, pane_id ile
```

`pane.focus` yön değil doğrudan pane ID alır (`pane.focus_direction` ayrı bir
metot) — bu yüzden v0.1'de bile ucuz, atlamaya gerek yok.

Pane kapandıysa listede olmaz; ayrıca state tutma.

## Shift+Enter — her zaman yeni sekme

Aynı hosta ikinci bir pencere istemek meşru bir ihtiyaç (biri log takip eder,
diğeri komut çalıştırır). `Shift+Enter` mevcut oturum taramasını atlar ve her
zaman yeni sekme açar. Düz `Enter` en eskisine döner.

**Ölçüldü:** terminaller `Shift+Enter`'ı varsayılan olarak düz `Enter`'a
katlar. Ayırt edilebilmesi için kitty keyboard protokolü gerekiyor:

```rust
PushKeyboardEnhancementFlags(DISAMBIGUATE_ESCAPE_CODES)
```

`ratatui::init()` bunu yapmıyor; açıkça push edilmeli ve çıkarken pop
edilmeli. Herdr 0.9.1 `supports_keyboard_enhancement = true` döndürüyor ve
push sonrası `Enter mods=SHIFT` geliyor. Push edilmezse `Enter mods=0`.

Desteklemeyen bir terminalde `Shift+Enter` düz `Enter` gibi davranır —
özellik kaybolur ama hiçbir şey bozulmaz.

---

# 11. Search

`/` ile fuzzy search. Alias, klasör adı ve note üzerinde çalışır.
Sonuçlar hiyerarşiden bağımsız düz liste olarak gösterilir.

Basit subsequence matching yeterli; skorlama ve fuzzy crate'i gerekmez.
Eşleşen karakterleri vurgulamak güzel ama v0.1 şartı değil.

---

# 12. Favorites

`f` ile toggle. Favorites en üstte sanal bölüm olarak gösterilir.
Host fiziksel olarak kendi klasöründeki konumunu korur; favorite bölümü
yalnızca shortcut'tır.

---

# 13. Host metadata

v0.1 için yeterli:

```text
ssh_alias  folder  favorite  note
```

Tags ileride. **Password kesinlikle saklanmaz.** Credential vault yazılmaz.
Bitwarden entegrasyonu kapsam dışı. Authentication tamamen OpenSSH/ssh-agent'a
bırakılır.

---

# 14. Folder yönetimi

Nested folder desteklenir. Model basit: `id`, `name`, `parent`.

Döngüsel parent referansı ve var olmayan parent hatalı config'tir; panic etme,
o klasörü köke al ve status line'da uyar.

v0.1'de klasör/host düzenleme doğrudan config dosyası üzerinden yapılır.
Interactive folder editor yoktur. Öncelik: `read + display + connect`.

---

# 15. Reload

`r` ile ssh config ve favorites yeniden okunur. File watcher yok.

Popup her açılışta zaten sıfırdan okur — `r` uzun süre açık kalan oturum için.

---

# 16. Status / ping

v0.1'de online/offline detection yok. ICMP ping'e güvenme; SSH sunucuları
ICMP'yi engelleyebilir. İleride TCP connect ile reachability check yapılabilir.

---

# 17. Kapsam dışı

```text
RDP  VNC  embedded terminal emulator  SFTP browser
password vault  credential encryption  cloud sync
Docker manager  server monitoring  CPU/RAM graphs
AWS discovery  Kubernetes  web UI  Electron  database
file watcher  sidebar docking (v0.2)
```

Bu proje Remote Desktop Manager klonu değildir.

---

# 18. Plugin lifecycle

Geliştirme sırasında:

```bash
herdr plugin link /path/to/herdr-hosts-plugin
```

`plugin link` build adımlarını çalıştırmaz — binary'yi elle `cargo build
--release` ile üret. GitHub kurulumu için `[[build]]` adımı eklenir.

Manifest iskeleti:

```toml
id = "herdr-hosts"
name = "SSH Hosts"
version = "0.1.0"
min_herdr_version = "0.9.0"
platforms = ["linux", "macos"]

[[panes]]
id = "picker"
title = "SSH Hosts"
placement = "popup"
width = "60%"
height = "70%"
command = ["./target/release/herdr-hosts"]

[[panes]]
id = "ssh"
title = "SSH"
placement = "split"
command = ["./target/release/herdr-hosts-ssh"]

[[actions]]
id = "open"
title = "SSH Hosts"
description = "Open the SSH host picker"
command = ["./target/release/herdr-hosts", "--open"]
```

`open` action'ı `plugin pane open --entrypoint picker` çağırır; §3.5'teki
keybinding buna bağlanır.

Plugin devre dışı bırakıldığında veya kaldırıldığında Herdr normal çalışmaya
devam eder — popup kalıcı state bırakmaz.

---

# 19. Geliştirme sırası

## Phase 1 — Dikey dilim ✅

```text
herdr plugin link
→ herdr plugin action invoke herdr-hosts.open
→ popup açılır, hardcoded host listesi görünür
→ Enter
→ sağda ssh pane'i açılır, ssh çalışır
→ popup kapanır
```

Bu çalışmadan folder/search/config'e geçme. Buradaki tek riskli nokta
`--target-pane`'in context'ten doğru okunması; onu erken doğrula.

## Phase 2 — SSH config ✅

`~/.ssh/config` + `Include`. Hardcoded hostu kaldır.

## Phase 3 — Inventory ✅

Folder metadata, `Ungrouped` bölümü, atomic yazma.

## Phase 4 — UX ✅

Search, favorites, help, reload, mevcut pane'i focus etme (§10).

## Phase 5 — Polish ✅

Hata yönetimi, README, release binary, keybinding dokümantasyonu.

---

# 20. Hata yönetimi

Şunlar kullanıcıya anlaşılır gösterilmeli:

```text
~/.ssh/config yok           → boş liste + "no hosts found" mesajı
başlıksız config            → tek düz liste (Ungrouped), uyarı yok
HERDR_ENV != 1             → "Herdr dışında çalışıyor" + çık
HERDR_PLUGIN_STATE_DIR yok  → devam, favorite kaydedilemez + status uyarısı
Herdr API erişilemiyor      → status line'da hata
pane açılamadı / ui_busy    → popup açık kalır, status line'da hata
ssh binary yok              → launcher anlaşılır mesajla çıkar
```

**TUI panic edip kapanmamalı.** Alt status line'da tek satır hata yeterli.

Panic hook kur: terminal raw mode'dan çık, sonra hatayı bas. Popup'ta panic
eden bir TUI terminali bozuk bırakır.

---

# 21. v0.2 — Sidebar (ertelendi)

Kalıcı sol dock sidebar isteniyorsa gereken ek işler:

* `placement = "split"` ile aç, `pane.swap --direction left` ile sola taşı
  (split yönü yalnızca `right`/`down`).
* Genişlik: `pane.resize` ratio tabanlı. Sütun → ratio dönüşümü
  `pane.layout`'tan gelen rect ve split ratio'ları üzerinden hesaplanmalı.
* Layout survival: `pane.focused`, `tab.created`, `tab.focused`,
  `workspace.created`, `workspace.focused` event hook'ları + idempotent
  `--ensure` + reentrancy kilidi.

v0.1'in TUI, parser, favorites ve search kodu aynen kullanılır. Bu yüzden
v0.1'de popup'a özel varsayım yapma: "hangi pane'i hedefleyeceğim" bilgisini
tek bir yerden (§3.1 context) al.

---

# 22. Testler

Unit test:

```text
ssh config parsing (Include, wildcard eleme, çoklu alias, case)
başlık ayrıştırma (hangi yorum başlık, hangisi değil)
satır sonu yorumu → not, alias değil
folder hierarchy construction (başlık yollarından)
favorite toggle + atomic yazma
search matching
alias doğrulama (kabul/red vakaları)
```

Herdr çağrılarını `herdr.rs` içinde topla — dosya sınırı yeterli soyutlama.
**`trait HerdrClient` + mock yazma.** Test edilen mantığın tamamı (parser,
favorites, hiyerarşi, search, doğrulama) zaten Herdr'dan bağımsız saf
fonksiyonlar. Gerçekten iki implementasyon gerekince trait ekle.

---

# 23. Kod kalitesi

Hedef: `small`, `boring`, `maintainable`.

```text
src/
├── main.rs         TUI entry, event loop, --open
├── ui.rs           ratatui render
├── app.rs          state, navigation, search
├── ssh_config.rs   host + folder + note, Include
├── favorites.rs    yıldızlı alias listesi
├── herdr.rs        CLI çağrıları (plugin pane open, pane list/focus/rename)
├── lib.rs          modül kökü + valid_alias (iki binary de kullanır)
└── bin/ssh.rs      HERDR_HOSTS_ALIAS → ssh
```

İhtiyaç yoksa bundan fazla parçalama. Yüzlerce satırlık generic architecture
oluşturma.

---

# 24. README

İçermeli: terminal capture, kurulum, prerequisites (Herdr ≥ 0.9.0, OpenSSH),
keybinding tanımı, örnek `~/.ssh/config`, örnek inventory, kısayollar,
mimari özeti, limitasyonlar.

Özellikle belirt:

> Credentials are never stored by the plugin. SSH authentication is delegated
> to OpenSSH.

MIT attribution: `alexarthurs/herdr-sidebar`'dan kod alındıysa belirt.

---

# 25. Başarı kriterleri

1. Herdr 0.9.1 üzerinde çalışır.
2. Herdr core değiştirilmemiştir.
3. Keybinding ile popup picker açılır.
4. `~/.ssh/config` hostları (Include, başlık, not dahil) okunur.
5. Hostlar nested folder altında organize edilebilir.
6. Başlıksız hostlar `Ungrouped` altında görünür.
7. Keyboard navigation çalışır.
8. Fuzzy search çalışır.
9. Favorites çalışır ve kalıcıdır.
10. Enter ile seçilen host Herdr pane'inde açılır; **hiçbir noktada shell
    interpolation yoktur**.
11. Aynı host zaten açıksa mevcut pane focus edilir.
12. SSH credential saklanmaz; `~/.ssh/config`'e hiç yazılmaz.
13. Plugin kaldırıldığında Herdr normal çalışır.
14. TUI hiçbir hata durumunda panic etmez.
15. Release tek `cargo build --release` ile üretilir.

---

# 26. En önemli ürün ilkesi

Her kararda şunu sor:

> Bu özellik kullanıcıyı SSH hostunu bulup bağlanmaya daha hızlı götürüyor mu?

Cevap hayırsa kapsam dışıdır.

Hedef:

```text
Herdr + hierarchical SSH bookmarks + fast keyboard navigation
```

Remote Desktop Manager veya Termius klonu değil. İlk çalışan sürümü
olabildiğince küçük tut.
