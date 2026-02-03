# Configure the Scaleway Provider using IAM Application credentials
provider "scaleway" {
  access_key      = var.scw_application_access_key
  secret_key      = var.scw_application_secret_key
  project_id      = var.scw_project_id
  organization_id = var.scw_organization_id
  region          = var.scw_region
  zone            = var.scw_zone
}

# Serverless SQL Database (PostgreSQL) - shared across environments
resource "scaleway_sdb_sql_database" "main" {
  name    = "typednotes"
  min_cpu = var.sdb_min_cpu
  max_cpu = var.sdb_max_cpu
  region  = var.scw_region
}

# Container Registry Namespace
resource "scaleway_registry_namespace" "main" {
  name        = "typednotes"
  description = "Container registry for TypedNotes"
  is_public   = false
  region      = var.scw_region
}

# Serverless Container Namespace
resource "scaleway_container_namespace" "main" {
  name        = "typednotes"
  description = "Serverless containers for TypedNotes"
  region      = var.scw_region

  secret_environment_variables = {
    DATABASE_URL = "postgres://${var.scw_application_id}:${var.scw_application_secret_key}@${scaleway_sdb_sql_database.main.endpoint}"
  }
}

# Serverless Container
resource "scaleway_container" "web" {
  name           = "web"
  namespace_id   = scaleway_container_namespace.main.id
  registry_image = var.container_image != "" ? var.container_image : "${scaleway_registry_namespace.main.endpoint}/web:latest"
  port           = 8080
  cpu_limit      = var.container_cpu_limit
  memory_limit   = var.container_memory_limit
  min_scale      = var.container_min_scale
  max_scale      = var.container_max_scale
  privacy        = "public"
  protocol       = "http1"
  deploy         = var.container_deploy
  region         = var.scw_region

  environment_variables = {
    RUST_LOG = "info"
  }

  secret_environment_variables = {
    DATABASE_URL              = "postgres://${var.scw_application_id}:${var.scw_application_secret_key}@${scaleway_sdb_sql_database.main.endpoint}"
    GOOGLE_CLIENT_ID          = var.google_client_id
    GOOGLE_CLIENT_SECRET      = var.google_client_secret
    GOOGLE_AUTH_REDIRECT_URI  = var.google_auth_redirect_uri
    GITHUB_CLIENT_ID          = var.github_client_id
    GITHUB_CLIENT_SECRET      = var.github_client_secret
    GITHUB_AUTH_REDIRECT_URI  = var.github_auth_redirect_uri
  }
}
