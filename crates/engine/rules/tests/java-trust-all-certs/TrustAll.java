import java.security.cert.X509Certificate;
import javax.net.ssl.X509TrustManager;

public class TrustAll implements X509TrustManager {
    // ruleid: java/trust-all-certs
    public void checkClientTrusted(X509Certificate[] chain, String authType) {}

    // ruleid: java/trust-all-certs
    public void checkServerTrusted(X509Certificate[] chain, String authType) {}

    public X509Certificate[] getAcceptedIssuers() {
        return new X509Certificate[0];
    }
}
