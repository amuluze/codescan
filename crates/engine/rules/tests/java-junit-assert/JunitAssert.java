import org.junit.Assert;

public class JunitAssert {
    public static void checkAdmin(boolean isAdmin) {
        // ruleid: java/junit-assert
        Assert.assertTrue("admin required", isAdmin);
        Assert.fail("not an admin path");
        done();
    }

    static void done() {
        System.out.println("checked");
    }
}
