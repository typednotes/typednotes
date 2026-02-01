# Output values

# Serverless SQL Database outputs
output "sdb_id" {
  description = "Serverless SQL Database ID"
  value       = scaleway_sdb_sql_database.main.id
}

output "sdb_endpoint" {
  description = "Serverless SQL Database endpoint (without credentials)"
  value       = scaleway_sdb_sql_database.main.endpoint
}

# Database credentials (from IAM Application)
output "sdb_username" {
  description = "Database username (IAM Application ID)"
  value       = var.scw_application_id
}

output "sdb_password" {
  description = "Database password (IAM Application secret key)"
  value       = var.scw_application_secret_key
  sensitive   = true
}
