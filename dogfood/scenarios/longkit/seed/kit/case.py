from kit._stub import stub


@stub
def to_snake(name: str) -> str:
    """camelCase or PascalCase to snake_case; a run of capitals is one word ('parseHTTPResponse' -> 'parse_http_response')."""
    raise NotImplementedError


@stub
def to_camel(name: str) -> str:
    """snake_case to camelCase ('hello_world' -> 'helloWorld')."""
    raise NotImplementedError
