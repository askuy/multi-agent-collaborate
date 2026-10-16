# Trellis

**Un espacio de trabajo compartido donde los agentes de IA coordinan tareas, integran cambios y corrigen errores juntos.**

[English](../../README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · [Deutsch](README.de.md) · [Русский](README.ru.md) · **Español** · [한국어](README.ko.md) · [Português](README.pt.md) · [Tiếng Việt](README.vi.md)

Trellis es un backend escrito en Rust para varios agentes de IA que trabajan sobre el mismo código, documentos o datos estructurados. Cada agente trabaja en su propia sesión; un servidor compartido conserva el historial, integra los cambios en `main` y avisa a los agentes cuando cambia el contenido del que depende su trabajo.

Puedes usarlo mediante **CLI**, **MCP**, **API HTTP** o **crates de Rust**. Tú proporcionas los agentes y su entorno de ejecución; Trellis aporta el estado compartido y la coordinación.

> **Estado: desarrollo inicial.** Ya están implementados el almacenamiento, la integración de cambios, la coordinación, la CLI, los servidores HTTP y MCP, el puente con Git y las comprobaciones posteriores a la integración. Hay resultados iniciales de rendimiento; las API y los flujos de trabajo pueden cambiar.

## Qué ofrece

| Función | Utilidad |
| --- | --- |
| Historial compartido | Conserva conjuntos de cambios versionados, asociados a su agente, sesión y tarea. |
| Fusiones y conflictos | Fusiona texto y cambios estructurados en JSON/YAML; conserva los conflictos pendientes para que los agentes los resuelvan. |
| Coordinación de tareas | Permite tomar tareas de forma atómica, declarar cambios previstos y usar permisos exclusivos con caducidad cuando hacen falta. |
| Contexto compartido | Registra hechos, fallos, resúmenes y decisiones; permite leer entradas nuevas y recibir avisos sobre cambios en contenido ya leído. |
| Validación | Ejecuta las comprobaciones configuradas después de integrar cambios en `main` y convierte los errores en tareas de reparación. |
| Compatibilidad con Git | Importa una instantánea de Git y exporta los conjuntos de cambios integrados como commits de Git. |

El ciclo habitual es **tomar una tarea → editar → integrar → responder a los resultados**. Aquí, *land* significa pedir al servidor que integre los cambios de una sesión en `main`. La validación se ejecuta después, por lo que una integración correcta no garantiza que las pruebas pasen.

## Inicio rápido

Necesitas **Rust 1.85+**, **Cargo** y **Git**. Los comandos siguientes usan una shell compatible con Bash.

### 1. Instalar

```bash
git clone https://github.com/askuy/multi-agent-collaborate.git
cd multi-agent-collaborate
cargo install --locked --path crates/trellis-cli
```

### 2. Iniciar el servidor compartido

En una terminal, ejecuta:

```bash
tl serve
```

La dirección predeterminada es `http://127.0.0.1:7420`. Los datos se guardan en `.trellis-data`, dentro del directorio de trabajo del servidor. Mantén esta terminal abierta. Todos los agentes de un repositorio deben conectarse al mismo proceso del servidor, ya que los bloqueos de coordinación residen en ese proceso.

### 3. Compartir un cambio entre dos agentes

En otra terminal, usa un directorio nuevo:

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

La copia de trabajo de Bob ahora contiene `Hello from Alice`. Para seguir trabajando después de integrar un cambio, abre la siguiente sesión con `tl --as alice next ./alice`. Los resultados de los comandos se imprimen en JSON; `land` devuelve el código de salida `2` si hay conflictos y `3` si se rechazan los cambios.

Consulta la [guía de uso](../usage.md), en inglés, para conocer las colas de tareas, actualizaciones, resolución de conflictos, políticas y validadores.

## Conectar un agente

| Interfaz | Punto de entrada | Uso habitual |
| --- | --- | --- |
| CLI | `tl` | Agentes con acceso a la shell, personas y scripts |
| MCP por stdio | `tl mcp` | Agentes compatibles con MCP |
| HTTP/JSON + SSE | `tl serve` | Aplicaciones en cualquier lenguaje |
| Crates de Rust | `crates/` | Integración dentro del proceso y clientes Rust |

Para Claude Code, inicia `tl serve`, inicializa el repositorio `demo` y añade lo siguiente al archivo `.mcp.json` del proyecto:

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

Asigna una identidad distinta a cada agente, por ejemplo `TRELLIS_PRINCIPAL=agent-1 claude`. Cada agente ejecuta su propio proceso `tl mcp`, conectado al servidor HTTP compartido. Configura `TRELLIS_SERVER` si el servidor está en otra dirección. Las herramientas MCP cubren sesiones, lectura y escritura, integración, tareas, conflictos, notificaciones y contexto compartido.

## Qué muestran las pruebas de rendimiento

La [comparación del 16 de octubre de 2026](../dogfood/2026-10-16-final-comparison.md) incluye tres ejecuciones por configuración con el mismo modelo, nivel de esfuerzo de razonamiento y herramientas. Son experimentos pequeños, no una garantía general de rendimiento.

Mediana del tiempo hasta que pasan todas las pruebas, incluidas las reparaciones necesarias:

| Configuración | 60 tareas | Cambio de interfaz |
| --- | --- | --- |
| Un agente | 8:52 | 1:10 |
| Git worktrees, fusión predeterminada | 3:52 | 3:21 |
| Git worktrees, fusión union para el registro de cambios | 3:14 | 1:51 |
| Trellis, sin bloqueos | 3:23 | 2:26 |

Los tiempos están en minutos:segundos. Las configuraciones paralelas usaron cinco agentes para las 60 tareas y tres para el cambio de interfaz. Trellis fue incorporando el trabajo terminado a `main` durante la ejecución. Para las tareas independientes, Git worktrees con fusión union del registro de cambios fue más rápido y barato; un solo agente fue el más rápido para el pequeño cambio de interfaz. El informe también incluye costes, reparaciones y resultados de Trellis con bloqueos.

## Documentación

- [Guía de uso](../usage.md) — flujo diario, configuración, puente con Git y validación.
- [Índice de documentación](../README.md) — diseño, investigación e informes de experimentos.
- [Especificación de diseño](../superpowers/specs/2026-10-15-trellis-design.md) — arquitectura, modelo de datos y planes.
- [Herramientas de evaluación](../../dogfood/) — escenarios y scripts de las comparaciones.
- [Fuentes de investigación](../../references/README.md) — referencias sobre control de versiones y coordinación de agentes.

## Contribuir

Se aceptan issues, informes de errores, experimentos reproducibles y pull requests. Lee la especificación de diseño para entender la arquitectura y ejecuta estas comprobaciones cuando cambies código:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Al actualizar un README, mantén las nueve versiones de idioma sincronizadas. La documentación técnica complementaria está actualmente, en su mayoría, en inglés.

## Licencia

Todavía no se ha elegido una licencia. El repositorio no incluye actualmente un archivo `LICENSE`.
