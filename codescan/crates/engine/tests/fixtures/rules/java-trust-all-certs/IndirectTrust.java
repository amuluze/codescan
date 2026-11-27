import java.security.cert.X509Certificate;

abstract class BaseTrustManager implements javax.net.ssl.X509TrustManager {}

public class IndirectTrust extends BaseTrustManager {
    // ruleid: java/trust-all-certs-inherited
    public void checkClientTrusted(X509Certificate[] chain, String authType) {}

    // ruleid: java/trust-all-certs-inherited
    public void checkServerTrusted(X509Certificate[] chain, String authType) {}

    // ruleid: java/trust-all-certs-inherited
    public void checkClientTrusted(X509Certificate[] chain, String authType, Object extra) {
        /* intentionally empty */
    }
}
