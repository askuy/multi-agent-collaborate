# Trellis

**Ein gemeinsamer Arbeitsbereich, in dem KI-Agenten Aufgaben koordinieren, Änderungen zusammenführen und Fehler gemeinsam beheben.**

[English](../../README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · **Deutsch** · [Русский](README.ru.md) · [Español](README.es.md) · [한국어](README.ko.md) · [Português](README.pt.md) · [Tiếng Việt](README.vi.md)

Trellis ist ein in Rust geschriebenes Backend für mehrere KI-Agenten, die am selben Code, an Dokumenten oder an strukturierten Daten arbeiten. Jeder Agent arbeitet in einer eigenen Sitzung. Ein gemeinsamer Server verwaltet die Historie, integriert Änderungen in `main` und benachrichtigt Agenten, wenn sich Inhalte ändern, auf denen ihre Arbeit beruht.

Der Zugriff erfolgt über **CLI**, **MCP**, **HTTP-API** oder **Rust-Crates**. Du stellst die Agenten und ihre Laufzeitumgebung bereit; Trellis übernimmt den gemeinsamen Zustand und die Koordination.

> **Status: frühe Entwicklung.** Speicherung, Zusammenführung, Koordination, CLI, HTTP- und MCP-Server, Git-Anbindung sowie Prüfungen nach der Integration sind implementiert. Erste Benchmarks liegen vor. APIs und Abläufe können sich noch ändern.

## Funktionen

| Funktion | Nutzen |
| --- | --- |
| Gemeinsame Historie | Versionierte Änderungssätze mit Zuordnung zu Agent, Sitzung und Aufgabe. |
| Zusammenführung und Konflikte | Text sowie strukturierte JSON/YAML-Änderungen zusammenführen; ungelöste Konflikte zur Bearbeitung durch Agenten erhalten. |
| Aufgabenkoordination | Aufgaben atomar übernehmen, geplante Änderungen ankündigen und bei Bedarf zeitlich begrenzte exklusive Zugriffsrechte nutzen. |
| Gemeinsamer Kontext | Fakten, Fehler, Zusammenfassungen und Entscheidungen festhalten; neue Einträge lesen und Benachrichtigungen zu betroffenen Lesezugriffen erhalten. |
| Validierung | Konfigurierte Prüfungen nach der Integration in `main` ausführen und aus Fehlern Reparaturaufgaben erstellen. |
| Git-Anbindung | Einen Git-Snapshot importieren und integrierte Änderungssätze als Git-Commits exportieren. |

Der übliche Ablauf ist **Aufgabe übernehmen → bearbeiten → integrieren → auf Rückmeldungen reagieren**. *Land* bedeutet hier, den Server um die Integration der Sitzungsänderungen in `main` zu bitten. Die Validierung folgt danach; eine erfolgreiche Integration bedeutet daher nicht, dass alle Tests bestehen.

## Schnellstart

Vorausgesetzt werden **Rust 1.85+**, **Cargo** und **Git**. Die folgenden Befehle verwenden eine Bash-kompatible Shell.

### 1. Installieren

```bash
git clone https://github.com/askuy/multi-agent-collaborate.git
cd multi-agent-collaborate
cargo install --locked --path crates/trellis-cli
```

### 2. Gemeinsamen Server starten

In einem Terminal ausführen:

```bash
tl serve
```

Die Standardadresse lautet `http://127.0.0.1:7420`. Daten werden unter `.trellis-data` im Arbeitsverzeichnis des Servers gespeichert. Lass dieses Terminal geöffnet. Alle Agenten eines Repositorys müssen sich mit demselben Serverprozess verbinden, da die Koordinationssperren in diesem Prozess liegen.

### 3. Eine Änderung zwischen zwei Agenten teilen

In einem weiteren Terminal ein neues Verzeichnis verwenden:

```bash
mkdir trellis-demo
cd trellis-demo
export TRELLIS_REPO=demo
tl init

tl --as alice open ./alice
echo "Hello from Alice" > ./alice/notes.md
tl --as alice land ./alice -m "Add a shared note"

tl --as bob open ./bob
cat ./bob/notes.md
```

Bobs Arbeitskopie enthält nun `Hello from Alice`. Nach der Integration öffnest du mit `tl --as alice next ./alice` die nächste Sitzung. Befehlsergebnisse werden als JSON ausgegeben. `land` liefert bei Konflikten den Exit-Code `2`, bei abgelehnten Änderungen `3`.

Aufgabenwarteschlangen, Aktualisierungen, Konfliktlösung, Richtlinien und Validatoren beschreibt die [Bedienungsanleitung](../usage.md) auf Englisch.

## Einen Agenten anbinden

| Schnittstelle | Einstieg | Typischer Einsatz |
| --- | --- | --- |
| CLI | `tl` | Agenten mit Shell-Zugriff, manuelle Arbeit und Skripte |
| MCP über stdio | `tl mcp` | Agenten mit MCP-Unterstützung |
| HTTP/JSON + SSE | `tl serve` | Anwendungen in beliebigen Sprachen |
| Rust-Crates | `crates/` | Einbettung in den eigenen Prozess und Rust-Clients |

Für Claude Code startest du zunächst `tl serve` und initialisierst das Repository `demo`. Ergänze dann die `.mcp.json` des Projekts:

```json
{
  "mcpServers": {
    "trellis": {
      "command": "tl",
      "args": ["mcp"],
      "env": {
        "TRELLIS_REPO": "demo"
      }
    }
  }
}
```

Gib jedem Agenten eine eigene Identität, etwa mit `TRELLIS_PRINCIPAL=agent-1 claude`. Jeder Agent verwendet einen eigenen `tl mcp`-Prozess, der mit dem gemeinsamen HTTP-Server verbunden ist. Für eine abweichende Serveradresse setzt du `TRELLIS_SERVER`. Die MCP-Werkzeuge decken Sitzungen, Lesen und Schreiben, Integration, Aufgaben, Konflikte, Benachrichtigungen und gemeinsamen Kontext ab.

## Was die Benchmarks zeigen

Der [Vergleich vom 16. Oktober 2026](../dogfood/2026-10-16-final-comparison.md) umfasst drei Durchläufe je Konfiguration mit demselben Modell, derselben Einstellung für den Denkaufwand und denselben Agentenwerkzeugen. Es handelt sich um kleine Experimente, nicht um eine allgemeine Leistungsgarantie.

Median der Zeit bis zum Bestehen aller Tests, einschließlich notwendiger Reparaturen:

| Konfiguration | 60 Aufgaben | Schnittstellenänderung |
| --- | --- | --- |
| Ein Agent | 8:52 | 1:10 |
| Git worktrees, Standard-Merge | 3:52 | 3:21 |
| Git worktrees, Union-Merge für das Änderungsprotokoll | 3:14 | 1:51 |
| Trellis, ohne Sperren | 3:23 | 2:26 |

Zeitangaben in Minuten:Sekunden. Die parallelen Konfigurationen nutzten fünf Agenten für die 60 Aufgaben und drei für die Schnittstellenänderung. Trellis stellte abgeschlossene Arbeit schon während des Durchlaufs auf `main` bereit. Bei unabhängigen Aufgaben waren Git worktrees mit Union-Merge für das Änderungsprotokoll schneller und günstiger; bei der kleinen Schnittstellenänderung war ein einzelner Agent am schnellsten. Der Bericht enthält auch Kosten, Reparaturabläufe und Ergebnisse für Trellis mit Sperren.

## Dokumentation

- [Bedienungsanleitung](../usage.md) — täglicher Ablauf, Konfiguration, Git-Anbindung und Validierung.
- [Dokumentationsübersicht](../README.md) — Entwurf, Forschung und Versuchsberichte.
- [Entwurfsspezifikation](../superpowers/specs/2026-10-15-trellis-design.md) — Architektur, Datenmodell und geplante Arbeiten.
- [Benchmark-Werkzeuge](../../dogfood/) — Szenarien und Skripte für die Vergleiche.
- [Quellen](../../references/README.md) — Versionsverwaltung und Koordination mehrerer Agenten.

## Mitwirken

Issues, Fehlerberichte, reproduzierbare Experimente und Pull Requests sind willkommen. Die Entwurfsspezifikation erläutert die Architektur. Führe bei Codeänderungen diese Prüfungen aus:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Halte bei Änderungen an einer README alle neun Sprachversionen auf demselben Stand. Die weiterführenden technischen Dokumente sind derzeit überwiegend auf Englisch.

## Lizenz

Eine Lizenz wurde noch nicht festgelegt. Das Repository enthält derzeit keine `LICENSE`-Datei.
