public class ReadObject {
    public static Object read(java.io.ObjectInputStream in) throws Exception {
        // ruleid: java/read-object
        Object value = in.readObject();
        int size = in.available();
        return value + size;
    }
}
