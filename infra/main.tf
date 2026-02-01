# Configure the Scaleway Provider using IAM Application credentials
provider "scaleway" {
  access_key      = var.scw_application_access_key
  secret_key      = var.scw_application_secret_key
  project_id      = var.scw_project_id
  organization_id = var.scw_organization_id
  region          = var.scw_region
  zone            = var.scw_zone
}

# Serverless SQL Database (PostgreSQL)
resource "scaleway_sdb_sql_database" "main" {
  name    = "typednotes-${var.environment}"
  min_cpu = var.sdb_min_cpu
  max_cpu = var.sdb_max_cpu
  region  = var.scw_region
}
