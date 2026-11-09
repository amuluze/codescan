class SpawnHandler {
  void run(String userInput) {
    // ruleid: java/taint-processbuilder-param
    // ruleid: java/process-builder
    new ProcessBuilder(userInput);
  }

  void sanitized(String raw) {
    int n = Integer.parseInt(raw);
    // ruleid: java/process-builder
    new ProcessBuilder("echo", String.valueOf(n));
  }

  void literal() {
    // ruleid: java/process-builder
    new ProcessBuilder("echo", "ok");
  }
}
