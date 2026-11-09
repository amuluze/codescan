import java.security.SecureRandom;
import java.util.Random;

public class InsecureRandom {
    public static String sessionToken() {
        // ruleid: java/insecure-random
        Random rnd = new Random();
        return Long.toHexString(rnd.nextLong());
    }

    public static long nonce() {
        SecureRandom secure = new SecureRandom();
        return secure.nextLong();
    }
}
