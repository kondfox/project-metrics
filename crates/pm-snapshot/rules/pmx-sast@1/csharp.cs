using System.Data.SqlClient;
using System.Runtime.Serialization.Formatters.Binary;

class Demo
{
    void Sql(SqlConnection conn, AppDb db, string id)
    {
        // ruleid: pmx-cs-sql-injection
        var a = new SqlCommand("SELECT * FROM Users WHERE Id = '" + id + "'", conn);
        // ruleid: pmx-cs-sql-injection
        db.Database.ExecuteSqlRaw($"DELETE FROM Users WHERE Id = {id}");
        // ok: pmx-cs-sql-injection
        var b = new SqlCommand("SELECT * FROM Users WHERE Id = @id", conn);
        // ok: pmx-cs-sql-injection
        db.Database.ExecuteSqlInterpolated($"DELETE FROM Users WHERE Id = {id}");
        // ok: pmx-cs-sql-injection
        Log("Hello " + id);
    }

    object Load(System.IO.Stream s)
    {
        // ruleid: pmx-cs-binaryformatter
        var f = new BinaryFormatter();
        return f.Deserialize(s);
    }

    void Tls(HttpClientHandler handler)
    {
        // ruleid: pmx-cs-tls-validation-disabled
        handler.ServerCertificateCustomValidationCallback = (msg, cert, chain, errors) => true;
        // ok: pmx-cs-tls-validation-disabled
        handler.ServerCertificateCustomValidationCallback = (msg, cert, chain, errors) => errors == SslPolicyErrors.None;
    }
}
