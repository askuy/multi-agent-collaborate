# Trellis

**AI 에이전트가 작업을 조율하고, 변경 사항을 병합하고, 문제를 함께 해결하는 공유 작업 공간입니다.**

[English](../../README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · [Deutsch](README.de.md) · [Русский](README.ru.md) · [Español](README.es.md) · **한국어** · [Português](README.pt.md) · [Tiếng Việt](README.vi.md)

Trellis는 여러 AI 에이전트가 같은 코드, 문서, 구조화된 데이터를 다룰 수 있도록 Rust로 작성한 백엔드입니다. 각 에이전트는 독립된 세션에서 작업합니다. 공유 서버는 이력을 보관하고 변경 사항을 `main`에 통합하며, 작업이 의존하는 내용이 바뀌면 관련 에이전트에 알립니다.

**CLI**, **MCP**, **HTTP API**, **Rust 크레이트**를 통해 사용할 수 있습니다. 사용자는 에이전트와 실행 환경을 제공하고, Trellis는 공유 상태와 작업 조율을 담당합니다.

> **개발 상태: 초기 단계.** 핵심 저장소, 병합, 작업 조율, CLI, HTTP 및 MCP 서버, Git 연동, 통합 후 검증 기능이 구현되어 있습니다. 초기 벤치마크 결과도 공개되어 있으며, API와 작업 흐름은 변경될 수 있습니다.

## 주요 기능

| 기능 | 역할 |
| --- | --- |
| 공유 이력 | 변경 집합의 버전을 관리하고 에이전트, 세션, 작업 정보를 연결합니다. |
| 병합과 충돌 처리 | 텍스트 및 JSON/YAML의 구조화된 변경을 병합하고, 해결되지 않은 충돌은 에이전트가 수정할 수 있도록 보존합니다. |
| 작업 조율 | 작업을 원자적으로 할당받고 수정 의도를 선언합니다. 독점 접근이 필요하면 만료 시간이 있는 임대를 사용합니다. |
| 공유 컨텍스트 | 사실, 실패, 요약, 결정을 기록합니다. 새 기록만 읽고, 이미 읽은 내용에 영향을 주는 변경 알림을 받습니다. |
| 검증 | 변경 사항이 `main`에 들어간 뒤 설정된 검사를 실행하고 오류를 수정 작업으로 전환합니다. |
| Git 연동 | Git 스냅샷을 가져오고 통합된 변경 집합을 Git 커밋으로 내보냅니다. |

일반적인 흐름은 **작업 가져오기 → 편집 → 통합 → 피드백 처리**입니다. 여기서 *land*는 세션의 변경 사항을 `main`에 통합하도록 서버에 요청하는 작업입니다. 검증은 그 이후에 실행되므로, 통합 성공이 테스트 통과를 의미하지는 않습니다.

## 빠른 시작

**Rust 1.85 이상**, **Cargo**, **Git**이 필요합니다. 아래 명령은 Bash와 호환되는 셸에서 실행합니다.

### 1. 설치

```bash
git clone https://github.com/askuy/multi-agent-collaborate.git
cd multi-agent-collaborate
cargo install --locked --path crates/trellis-cli
```

### 2. 공유 서버 시작

터미널 하나에서 실행합니다.

```bash
tl serve
```

기본 주소는 `http://127.0.0.1:7420`입니다. 데이터는 서버 작업 디렉터리의 `.trellis-data`에 저장됩니다. 이 터미널은 계속 열어 두세요. 조율에 사용하는 잠금이 프로세스 안에 저장되므로, 같은 저장소를 사용하는 모든 에이전트는 동일한 서버 프로세스에 연결해야 합니다.

### 3. 두 에이전트 사이에서 변경 사항 공유

다른 터미널에서 새 디렉터리를 사용합니다.

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

이제 Bob의 작업 사본에 `Hello from Alice`가 들어 있습니다. 통합 후 계속 작업하려면 `tl --as alice next ./alice`로 다음 세션을 엽니다. 명령 결과는 JSON으로 출력됩니다. `land`의 종료 코드는 충돌 시 `2`, 변경 거부 시 `3`입니다.

작업 큐, 업데이트, 충돌 해결, 정책, 검증기에 관한 자세한 내용은 [사용 가이드](../usage.md)(영문)를 참고하세요.

## 에이전트 연결

| 인터페이스 | 진입점 | 주요 용도 |
| --- | --- | --- |
| CLI | `tl` | 셸을 사용할 수 있는 에이전트, 수동 작업, 스크립트 |
| stdio 기반 MCP | `tl mcp` | MCP를 지원하는 에이전트 |
| HTTP/JSON + SSE | `tl serve` | 모든 언어로 작성한 애플리케이션 |
| Rust 크레이트 | `crates/` | 프로세스 내 통합 및 Rust 클라이언트 |

Claude Code에서는 `tl serve`를 시작하고 `demo` 저장소를 초기화한 뒤, 프로젝트의 `.mcp.json`에 다음 설정을 추가합니다.

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

`TRELLIS_PRINCIPAL=agent-1 claude`처럼 각 에이전트에 서로 다른 식별자를 지정하세요. 각 에이전트는 독립된 `tl mcp` 프로세스를 실행하고 공유 HTTP 서버에 연결합니다. 서버 주소가 다르면 `TRELLIS_SERVER`를 설정합니다. MCP 도구로 세션, 읽기와 쓰기, 통합, 작업, 충돌, 알림, 공유 컨텍스트를 다룰 수 있습니다.

## 벤치마크 결과

[2026년 10월 16일 비교 보고서](../dogfood/2026-10-16-final-comparison.md)는 같은 모델, 추론 강도, 에이전트 도구를 사용해 각 구성을 세 번 실행한 결과입니다. 소규모 실험이므로 일반적인 성능을 보장하지는 않습니다.

필요한 수정 시간을 포함해 모든 테스트가 통과할 때까지 걸린 시간의 중앙값입니다.

| 구성 | 작업 60개 | 인터페이스 변경 |
| --- | --- | --- |
| 에이전트 1개 | 8:52 | 1:10 |
| Git worktrees, 기본 병합 | 3:52 | 3:21 |
| Git worktrees, 변경 기록에 union 병합 적용 | 3:14 | 1:51 |
| Trellis, 잠금 없음 | 3:23 | 2:26 |

시간 단위는 분:초입니다. 병렬 구성에서는 작업 60개에 에이전트 5개, 인터페이스 변경에 에이전트 3개를 사용했습니다. Trellis는 실행 중에도 완료된 작업을 `main`에서 사용할 수 있게 했습니다. 독립적인 작업에서는 변경 기록에 union 병합을 적용한 Git worktrees가 더 빠르고 저렴했고, 작은 인터페이스 변경에서는 단일 에이전트가 가장 빨랐습니다. 전체 보고서에는 비용, 수정 과정, 잠금을 사용한 Trellis 결과도 포함되어 있습니다.

## 문서

- [사용 가이드](../usage.md) — 일상 작업, 설정, Git 연동, 검증.
- [문서 목록](../README.md) — 설계, 연구, 실험 보고서.
- [설계 명세](../superpowers/specs/2026-10-15-trellis-design.md) — 아키텍처, 데이터 모델, 향후 계획.
- [벤치마크 도구](../../dogfood/) — 비교에 사용한 시나리오와 스크립트.
- [참고 자료](../../references/README.md) — 버전 관리 및 다중 에이전트 조율 관련 자료.

## 기여하기

이슈, 버그 보고, 재현 가능한 실험, 풀 리퀘스트를 환영합니다. 설계 명세에서 아키텍처를 확인하고, 코드를 변경했다면 다음 검사를 실행하세요.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

README를 수정할 때는 아홉 가지 언어 버전의 내용을 함께 맞춰 주세요. 관련 기술 문서는 현재 주로 영어로 제공됩니다.

## 라이선스

라이선스는 아직 정해지지 않았습니다. 현재 저장소에는 `LICENSE` 파일이 없습니다.
