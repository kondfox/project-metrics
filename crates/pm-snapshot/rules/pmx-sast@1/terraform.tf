resource "aws_s3_bucket_acl" "assets" {
  bucket = aws_s3_bucket.assets.id
  # ruleid: pmx-tf-public-bucket-acl
  acl = "public-read"
}

resource "aws_s3_bucket_acl" "logs" {
  bucket = aws_s3_bucket.logs.id
  # ok: pmx-tf-public-bucket-acl
  acl = "private"
}

resource "aws_security_group" "ssh" {
  ingress {
    from_port = 22
    to_port   = 22
    protocol  = "tcp"
    # ruleid: pmx-tf-admin-port-open-to-world
    cidr_blocks = ["0.0.0.0/0"]
  }
  ingress {
    from_port = 443
    to_port   = 443
    protocol  = "tcp"
    # ok: pmx-tf-admin-port-open-to-world
    cidr_blocks = ["0.0.0.0/0"]
  }
  ingress {
    from_port = 22
    to_port   = 22
    protocol  = "tcp"
    # ok: pmx-tf-admin-port-open-to-world
    cidr_blocks = ["10.0.0.0/8"]
  }
}
