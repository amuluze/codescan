import java.security.MessageDigest;

public class WeakDigest {
    public static byte[] hash(byte[] data) throws Exception {
        // ruleid: java/weak-message-digest
        MessageDigest md = MessageDigest.getInstance("MD5");
        return md.digest(data);
    }

    public static byte[] legacy(byte[] data) throws Exception {
        // ruleid: java/weak-message-digest
        MessageDigest md = MessageDigest.getInstance("SHA-1");
        return md.digest(data);
    }

    public static byte[] strong(byte[] data) throws Exception {
        MessageDigest md = MessageDigest.getInstance("SHA-256");
        return md.digest(data);
    }
}
