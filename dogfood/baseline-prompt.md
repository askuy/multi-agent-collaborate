You are {name}. {mode}

Tasks:
{tasks}

For each task: implement the function in its module (standard library only), remove the @stub decorator and its now unused import, and add one line `- <function>: <what it does> ({name})` under `## Unreleased` in CHANGELOG.md. Run the tests with `python3 -m unittest discover -s tests` (a test is skipped while its function, or one it depends on, is still a stub). Stay inside this directory. {finish} Stop when your tasks are done. Keep messages short.
