import java.io.*;
import java.sql.*;
import javax.net.ssl.*;

class Demo {
    void sql(Connection c, EntityManager em, String id) throws Exception {
        Statement s = c.createStatement();
        // ruleid: pmx-java-sql-injection
        s.executeQuery("SELECT * FROM users WHERE id = '" + id + "'");
        // ruleid: pmx-java-sql-injection
        em.createQuery("SELECT u FROM User u WHERE u.name = '" + id + "'");
        // ok: pmx-java-sql-injection
        PreparedStatement p = c.prepareStatement("SELECT * FROM users WHERE id = ?");
        // ok: pmx-java-sql-injection
        log.info("Hello " + id);
    }

    void run(String name) throws Exception {
        // ruleid: pmx-java-command-injection
        Runtime.getRuntime().exec("git log " + name);
        // ok: pmx-java-command-injection
        Runtime.getRuntime().exec("git status");
        // ok: pmx-java-command-injection
        new ProcessBuilder("git", "log", name).start();
    }

    Object read(InputStream in) throws Exception {
        // ruleid: pmx-java-unsafe-deserialization
        return new ObjectInputStream(in).readObject();
    }

    void tls(HttpsURLConnection conn) {
        // ruleid: pmx-java-tls-trust-all
        conn.setHostnameVerifier((host, session) -> true);
        // ok: pmx-java-tls-trust-all
        conn.setHostnameVerifier(HttpsURLConnection.getDefaultHostnameVerifier());
    }
}
