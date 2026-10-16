# Trellis

**Um espaço de trabalho compartilhado para agentes de IA coordenarem tarefas, integrarem alterações e corrigirem falhas juntos.**

[English](../../README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · [Deutsch](README.de.md) · [Русский](README.ru.md) · [Español](README.es.md) · [한국어](README.ko.md) · **Português** · [Tiếng Việt](README.vi.md)

Trellis é um backend escrito em Rust para vários agentes de IA que trabalham no mesmo código, em documentos ou em dados estruturados. Cada agente trabalha em sua própria sessão; um servidor compartilhado mantém o histórico, integra as alterações em `main` e avisa os agentes quando muda algum conteúdo do qual seu trabalho depende.

Você pode usar **CLI**, **MCP**, **API HTTP** ou **crates Rust**. Você fornece os agentes e o ambiente de execução; o Trellis oferece o estado compartilhado e a coordenação.

> **Status: desenvolvimento inicial.** Armazenamento, integração de alterações, coordenação, CLI, servidores HTTP e MCP, integração com Git e verificações após a integração estão implementados. Já há resultados iniciais de benchmarks; APIs e fluxos de trabalho ainda podem mudar.

## Funcionalidades

| Recurso | Utilidade |
| --- | --- |
| Histórico compartilhado | Mantém conjuntos de alterações versionados, associados ao agente, à sessão e à tarefa. |
| Mesclagem e conflitos | Mescla texto e alterações estruturadas em JSON/YAML; preserva conflitos pendentes para os agentes resolverem. |
| Coordenação de tarefas | Permite assumir tarefas de forma atômica, declarar alterações planejadas e usar permissões exclusivas com prazo de validade quando necessário. |
| Contexto compartilhado | Registra fatos, falhas, resumos e decisões; permite ler novas entradas e receber avisos sobre alterações em conteúdo já lido. |
| Validação | Executa verificações configuradas após as alterações chegarem a `main` e transforma erros em tarefas de correção. |
| Integração com Git | Importa um snapshot do Git e exporta conjuntos de alterações integrados como commits Git. |

O ciclo habitual é **assumir uma tarefa → editar → integrar → responder aos resultados**. Aqui, *land* significa pedir ao servidor que integre as alterações de uma sessão em `main`. A validação acontece depois, portanto uma integração bem-sucedida não garante que os testes passem.

## Início rápido

É necessário ter **Rust 1.85+**, **Cargo** e **Git**. Os comandos abaixo usam um shell compatível com Bash.

### 1. Instalar

```bash
git clone https://github.com/askuy/multi-agent-collaborate.git
cd multi-agent-collaborate
cargo install --locked --path crates/trellis-cli
```

### 2. Iniciar o servidor compartilhado

Em um terminal, execute:

```bash
tl serve
```

O endereço padrão é `http://127.0.0.1:7420`. Os dados ficam em `.trellis-data`, no diretório de trabalho do servidor. Mantenha esse terminal aberto. Todos os agentes de um repositório devem se conectar ao mesmo processo do servidor, pois os bloqueios de coordenação ficam nesse processo.

### 3. Compartilhar uma alteração entre dois agentes

Em outro terminal, use um diretório novo:

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

A cópia de trabalho de Bob agora contém `Hello from Alice`. Para continuar depois de integrar uma alteração, abra a próxima sessão com `tl --as alice next ./alice`. Os resultados dos comandos são exibidos em JSON. O comando `land` retorna o código de saída `2` em caso de conflito e `3` quando as alterações são rejeitadas.

Consulte o [guia de uso](../usage.md), em inglês, para saber mais sobre filas de tarefas, atualizações, resolução de conflitos, políticas e validadores.

## Conectar um agente

| Interface | Ponto de entrada | Uso típico |
| --- | --- | --- |
| CLI | `tl` | Agentes com acesso ao shell, pessoas e scripts |
| MCP via stdio | `tl mcp` | Agentes compatíveis com MCP |
| HTTP/JSON + SSE | `tl serve` | Aplicações em qualquer linguagem |
| Crates Rust | `crates/` | Integração dentro do processo e clientes Rust |

Para Claude Code, inicie `tl serve`, inicialize o repositório `demo` e adicione o seguinte ao `.mcp.json` do projeto:

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

Defina uma identidade diferente para cada agente, por exemplo `TRELLIS_PRINCIPAL=agent-1 claude`. Cada agente executa seu próprio processo `tl mcp`, conectado ao servidor HTTP compartilhado. Defina `TRELLIS_SERVER` caso o servidor esteja em outro endereço. As ferramentas MCP abrangem sessões, leitura e escrita, integração, tarefas, conflitos, notificações e contexto compartilhado.

## O que os benchmarks mostram

A [comparação de 16 de outubro de 2026](../dogfood/2026-10-16-final-comparison.md) apresenta três execuções por configuração com o mesmo modelo, nível de esforço de raciocínio e ferramentas dos agentes. São experimentos pequenos, não uma garantia geral de desempenho.

Mediana do tempo até todos os testes passarem, incluindo correções necessárias:

| Configuração | 60 tarefas | Alteração de interface |
| --- | --- | --- |
| Um agente | 8:52 | 1:10 |
| Git worktrees, mesclagem padrão | 3:52 | 3:21 |
| Git worktrees, mesclagem union para o changelog | 3:14 | 1:51 |
| Trellis, sem bloqueios | 3:23 | 2:26 |

Os tempos estão em minutos:segundos. As configurações paralelas usaram cinco agentes para as 60 tarefas e três para a alteração de interface. O Trellis disponibilizou o trabalho concluído em `main` ao longo da execução. Nas tarefas independentes, Git worktrees com mesclagem union do changelog foi mais rápido e barato; um único agente foi o mais rápido na pequena alteração de interface. O relatório também apresenta custos, o processo de correção e resultados do Trellis com bloqueios.

## Documentação

- [Guia de uso](../usage.md) — fluxo diário, configuração, integração com Git e validação.
- [Índice da documentação](../README.md) — projeto, pesquisa e relatórios de experimentos.
- [Especificação de projeto](../superpowers/specs/2026-10-15-trellis-design.md) — arquitetura, modelo de dados e planos.
- [Ferramentas de benchmark](../../dogfood/) — cenários e scripts usados nas comparações.
- [Fontes de pesquisa](../../references/README.md) — referências sobre controle de versão e coordenação de agentes.

## Como contribuir

Issues, relatos de bugs, experimentos reproduzíveis e pull requests são bem-vindos. Leia a especificação de projeto para entender a arquitetura e execute estas verificações ao alterar código:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Ao atualizar um README, mantenha as nove versões de idioma alinhadas. A documentação técnica complementar está atualmente, em sua maioria, em inglês.

## Licença

Uma licença ainda não foi escolhida. O repositório não contém um arquivo `LICENSE` no momento.
