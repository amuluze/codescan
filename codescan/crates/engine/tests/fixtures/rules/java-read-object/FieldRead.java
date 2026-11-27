import java.io.ObjectInputStream;

public class FieldRead {
    private ObjectInputStream input;

    Object readField() throws Exception {
        // ruleid: java/read-object-field
        return input.readObject();
    }

    ObjectInputStream stream() {
        return input;
    }

    void custom(FieldRead other) {
        other.readObject();
    }
}
