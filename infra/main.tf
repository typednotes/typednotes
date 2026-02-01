# Configure the Scaleway Provider
provider "scaleway" {
  access_key = var.scw_access_key
  secret_key = var.scw_secret_key
  project_id = var.scw_project_id
  region     = var.scw_region
  zone       = var.scw_zone
}

# Example: Container namespace for serverless containers
# Uncomment and modify as needed
#
# resource "scaleway_container_namespace" "main" {
#   name        = "typednotes-${var.environment}"
#   description = "TypedNotes container namespace"
# }

# Example: Object storage bucket
# Uncomment and modify as needed
#
# resource "scaleway_object_bucket" "main" {
#   name = "typednotes-${var.environment}"
#   acl  = "private"
# }

# Serverless SQL Database (PostgreSQL)
resource "scaleway_sdb_sql_database" "main" {
  name    = "typednotes-${var.environment}"
  min_cpu = var.sdb_min_cpu
  max_cpu = var.sdb_max_cpu
  region  = var.scw_region
}
