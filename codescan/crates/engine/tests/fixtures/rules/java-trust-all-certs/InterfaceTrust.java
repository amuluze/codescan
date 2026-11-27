import java.security.cert.X509Certificate;

interface TrustIface extends javax.net.ssl.X509TrustManager {}

class ViaInterface implements TrustIface {
    // ruleid: java/trust-all-certs-inherited
    public void checkClientTrusted(X509Certificate[] chain, String authType) {}

    // ruleid: java/trust-all-certs-inherited
    public void checkServerTrusted(X509Certificate[] chain, String authType) {}
}
