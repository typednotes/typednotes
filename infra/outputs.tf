# Output values

# Serverless SQL Database outputs
output "sdb_endpoint" {
  description = "Serverless SQL Database endpoint (with credentials)"
  value       = replace(scaleway_sdb_sql_database.main.endpoint, "postgres://", "postgres://${var.db_application_id}:${var.scw_secret_key}@")
  sensitive   = true
}

output "sdb_id" {
  description = "Serverless SQL Database ID"
  value       = scaleway_sdb_sql_database.main.id
}
