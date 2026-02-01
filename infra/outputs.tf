# Output values

# Serverless SQL Database outputs
output "sdb_endpoint" {
  description = "Serverless SQL Database endpoint"
  value       = scaleway_sdb_sql_database.main.endpoint
}

output "sdb_id" {
  description = "Serverless SQL Database ID"
  value       = scaleway_sdb_sql_database.main.id
}

# output "container_namespace_id" {
#   description = "Container namespace ID"
#   value       = scaleway_container_namespace.main.id
# }

# output "bucket_endpoint" {
#   description = "Object storage bucket endpoint"
#   value       = scaleway_object_bucket.main.endpoint
# }
