设计这样的CLI工具，用Rust编写：
  1. 尝试从 ~/.config/ccr-lite/config.toml 读取config
  2. 解析该toml文件，文件的架构应该类似：
  ```toml
  settings_file = "~/.claude/settings.json"

  [fox] # fox是名字
  url = "some url" # ANTHROPIC_BASE_URL
  auth = "sk-xxxx" # auth

  [fox.env]
  ANTHROPIC_SMALL_FAST_MOEL=Haiku

  [dog]
  url = "xxx"
  auth = "$DOG_URL"

  ```

  其中auth 在rust 中使用一个enum定义，直接指定和环境变量两种方式。如果以$开头则判断为环境变量，使用os.env.get()来获取其字面值

  3. 命令行：
  3.1 ccrl set fox :
  - 打开settings_file指定的文件（默认~/.claude/settings.json)，进行解析。其大概的结构应该类似   
     1 │ {
     2 │   "model": "sonnet",
     3 │   "env": {
     6 │   },
     7 │   "enabledPlugins": {
     8 │     "rust-analyzer-lsp@claude-plugins-official": true,
     9 │     "frontend-design@claude-plugins-official": true,
    10 │     "playwright@claude-plugins-official": true,
    11 │     "example-skills@anthropic-agent-skills": true,
    12 │     "swift-lsp@claude-plugins-official": true
    13 │   },
    14 │   "alwaysThinkingEnabled": true
    15 │ }
  （可能没有"env"这一项，这是我的配置)

  将[fox]指定的url 注入到"env":{}里面的 "ANTHROPIC_BASE_URL", auth 到
  "ANTHROPIC_AUTH_TOKEN", 如果有[fox.env]，直接全部key value 注入。

  3.2 ccrl now:
  output: fox

  3.3 ccrl fox:
  与3.1类似，但是不是注入到settings里,而是直接将这几个环境变量export到当前shell
