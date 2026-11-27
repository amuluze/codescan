package w;

import a.*;
import java.security.cert.X509Certificate;

public class WildcardTrust extends BasePkg {
    // ruleid: java/trust-all-certs-inherited
    public void checkClientTrusted(X509Certificate[] chain, String authType) {}

    // ruleid: java/trust-all-certs-inherited
    public void checkServerTrusted(X509Certificate[] chain, String authType) {}
}
