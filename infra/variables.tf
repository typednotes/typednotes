# Scaleway credentials
variable "scw_access_key" {
  description = "Scaleway access key"
  type        = string
  sensitive   = true
}

variable "scw_secret_key" {
  description = "Scaleway secret key"
  type        = string
  sensitive   = true
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

# Environment
variable "environment" {
  description = "Environment name (e.g., dev, staging, prod)"
  type        = string
  default     = "dev"
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
