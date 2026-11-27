import java.net.URL;

public class Ssrf {
    Object fetch(String target) throws Exception {
        // ruleid: java/url-open-connection
        return new URL(target).openConnection();
    }
}
