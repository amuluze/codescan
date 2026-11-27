import java.security.cert.X509Certificate;

public class IndirectFile extends Base2 {
    // ruleid: java/trust-all-certs-inherited
    public void checkClientTrusted(X509Certificate[] chain, String authType) {}

    // ruleid: java/trust-all-certs-inherited
    public void checkServerTrusted(X509Certificate[] chain, String authType) {}
}
