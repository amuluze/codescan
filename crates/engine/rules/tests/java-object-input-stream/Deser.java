import java.io.InputStream;
import java.io.ObjectInputStream;

public class Deser {
    public static ObjectInputStream open(InputStream in) throws Exception {
        // ruleid: java/object-input-stream
        ObjectInputStream ois = new ObjectInputStream(in);
        return ois;
    }
}
