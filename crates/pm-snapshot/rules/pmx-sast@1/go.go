package demo

import (
	"context"
	"crypto/md5"
	"crypto/sha256"
	"crypto/tls"
	"database/sql"
	"fmt"
	"os/exec"
)

func queries(ctx context.Context, db *sql.DB, id string) {
	// ruleid: pmx-go-sql-injection
	db.Query(fmt.Sprintf("SELECT * FROM users WHERE id = '%s'", id))
	// ruleid: pmx-go-sql-injection
	db.ExecContext(ctx, "DELETE FROM users WHERE id = "+id)
	// ruleid: pmx-go-sql-injection
	db.Query("SELECT * FROM users WHERE name = '" + id + "'")
	// ok: pmx-go-sql-injection
	db.Query("SELECT * FROM users WHERE id = $1", id)
	// ok: pmx-go-sql-injection
	fmt.Println(fmt.Sprintf("hello %s", id))
}

func clients() *tls.Config {
	// ruleid: pmx-go-tls-insecure-skip-verify
	bad := &tls.Config{MinVersion: tls.VersionTLS12, InsecureSkipVerify: true}
	// ok: pmx-go-tls-insecure-skip-verify
	good := &tls.Config{MinVersion: tls.VersionTLS12}
	_ = good
	return bad
}

func commands(name string) {
	// ruleid: pmx-go-command-injection
	exec.Command("sh", "-c", "git log "+name).Run()
	// ok: pmx-go-command-injection
	exec.Command("git", "log", name).Run()
	// ok: pmx-go-command-injection
	exec.Command("sh", "-c", "make build").Run()
}

func hashes(b []byte) {
	// ruleid: pmx-go-weak-hash
	md5.Sum(b)
	// ok: pmx-go-weak-hash
	sha256.Sum256(b)
}
