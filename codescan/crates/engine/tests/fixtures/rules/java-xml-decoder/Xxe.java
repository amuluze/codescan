import java.beans.XMLDecoder;
import java.io.InputStream;

public class Xxe {
    Object read(InputStream input) {
        // ruleid: java/xml-decoder
        XMLDecoder decoder = new XMLDecoder(input);
        return decoder.readObject();
    }
}
