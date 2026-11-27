import java.security.cert.X509Certificate;
import javax.net.ssl.X509TrustManager;

abstract class BaseAnon implements X509TrustManager {}

public class AnonymousIndirect {
    X509TrustManager manager = new BaseAnon() {
        // ruleid: java/trust-all-certs-inherited
        public void checkClientTrusted(X509Certificate[] chain, String authType) {}

        // ruleid: java/trust-all-certs-inherited
        public void checkServerTrusted(X509Certificate[] chain, String authType) {}
    };
}
