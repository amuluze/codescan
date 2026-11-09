class ExecHandler {
  void run(String userInput) {
    // ruleid: java/taint-exec-param
    // ruleid: java/runtime-exec
    Runtime.getRuntime().exec(userInput);
  }

  void sanitized(String raw) {
    int n = Integer.parseInt(raw);
    // ruleid: java/runtime-exec
    Runtime.getRuntime().exec(String.valueOf(n));
  }

  void externalOnly() {
    // ruleid: java/runtime-exec
    Runtime.getRuntime().exec(externalCode);
  }
}
