# Scaleway IAM Application credentials (service account)
# These credentials are used for both Scaleway API access and database authentication
variable "scw_application_id" {
  description = "Scaleway IAM Application ID (UUID)"
  type        = string
}

variable "scw_application_access_key" {
  description = "Scaleway IAM Application access key"
  type        = string
  sensitive   = true
}

variable "scw_application_secret_key" {
  description = "Scaleway IAM Application secret key (also used for database authentication)"
  type        = string
  sensitive   = true
}

# Scaleway organization and project
variable "scw_organization_id" {
  description = "Scaleway organization ID"
  type        = string
}

variable "scw_project_id" {
  description = "Scaleway project ID"
  type        = string
}

# Region and zone configuration
variable "scw_region" {
  description = "Scaleway region"
  type        = string
  default     = "fr-par"
}

variable "scw_zone" {
  description = "Scaleway zone"
  type        = string
  default     = "fr-par-1"
}

# Serverless SQL Database configuration
variable "sdb_min_cpu" {
  description = "Minimum CPU units for the serverless database (0 = scale to zero)"
  type        = number
  default     = 0
}

variable "sdb_max_cpu" {
  description = "Maximum CPU units for the serverless database (1-15)"
  type        = number
  default     = 4
}

# Serverless Container configuration
variable "container_image" {
  description = "Container image to deploy (must be public)"
  type        = string
  default     = "ghcr.io/typednotes/typednotes:latest"
}

variable "container_cpu_limit" {
  description = "CPU limit for the container in millicores (70-1120)"
  type        = number
  default     = 140
}

variable "container_memory_limit" {
  description = "Memory limit for the container in MB (128-4096)"
  type        = number
  default     = 256
}

variable "container_min_scale" {
  description = "Minimum number of container instances (0 = scale to zero)"
  type        = number
  default     = 0
}

variable "container_max_scale" {
  description = "Maximum number of container instances"
  type        = number
  default     = 5
}

variable "container_deploy" {
  description = "Whether to deploy the container immediately"
  type        = bool
  default     = true
}

# Domain configuration
variable "domain_name" {
  description = "Custom domain name for the application"
  type        = string
  default     = "typednotes.org"
}

# OAuth configuration
variable "google_client_id" {
  description = "Google OAuth Client ID"
  type        = string
  default     = ""
}

variable "google_client_secret" {
  description = "Google OAuth Client Secret"
  type        = string
  sensitive   = true
  default     = ""
}

variable "github_client_id" {
  description = "GitHub OAuth Client ID"
  type        = string
  default     = ""
}

variable "github_client_secret" {
  description = "GitHub OAuth Client Secret"
  type        = string
  sensitive   = true
  default     = ""
}

variable "encryption_key" {
  description = "AES-256-GCM master encryption key (64 hex chars / 32 bytes)"
  type        = string
  sensitive   = true
  default     = ""
}
