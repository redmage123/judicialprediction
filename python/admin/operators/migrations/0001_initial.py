"""
Initial migration for the operators app — creates operators_operator table.

After Django creates the table, grants are applied to jp_app and jp_admin via
DO blocks so the migration is safe to run in environments where those roles do
not yet exist (e.g., CI test databases).
"""

import uuid

import django.utils.timezone
from django.db import migrations, models


class Migration(migrations.Migration):
    initial = True
    dependencies = []

    operations = [
        migrations.CreateModel(
            name="Operator",
            fields=[
                (
                    "id",
                    models.UUIDField(
                        default=uuid.uuid4,
                        editable=False,
                        primary_key=True,
                        serialize=False,
                    ),
                ),
                (
                    "email",
                    models.EmailField(
                        help_text="Must match the Django auth user email.",
                        max_length=254,
                        unique=True,
                    ),
                ),
                (
                    "tenant_id",
                    models.UUIDField(
                        blank=True,
                        help_text="Scoped tenant UUID.  NULL only for role='super'.",
                        null=True,
                    ),
                ),
                (
                    "role",
                    models.CharField(
                        choices=[
                            ("admin", "Admin"),
                            ("viewer", "Viewer"),
                            ("super", "Super"),
                        ],
                        default="viewer",
                        max_length=10,
                    ),
                ),
                ("is_active", models.BooleanField(default=True)),
                ("created_at", models.DateTimeField(auto_now_add=True)),
                ("updated_at", models.DateTimeField(auto_now=True)),
            ],
            options={
                "verbose_name": "Operator",
                "verbose_name_plural": "Operators",
                "db_table": "operators_operator",
                "ordering": ["email"],
            },
        ),
        migrations.AddConstraint(
            model_name="operator",
            constraint=models.CheckConstraint(
                # role='super' implies tenant_id IS NULL.
                # ~Q(role='super') covers admin/viewer (may have any tenant_id).
                check=~models.Q(role="super") | models.Q(tenant_id__isnull=True),
                name="super_implies_null_tenant",
            ),
        ),
        # Grant DML to jp_app and jp_admin if those roles exist.
        # Uses DO blocks so the migration is idempotent in test environments
        # where the roles are not created by the Rust migration runner.
        migrations.RunSQL(
            sql="""
                DO $$
                BEGIN
                    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'jp_app') THEN
                        GRANT SELECT, INSERT, UPDATE, DELETE
                            ON TABLE operators_operator TO jp_app;
                    END IF;
                    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'jp_admin') THEN
                        GRANT SELECT, INSERT, UPDATE, DELETE
                            ON TABLE operators_operator TO jp_admin;
                    END IF;
                END$$;
            """,
            reverse_sql="""
                DO $$
                BEGIN
                    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'jp_app') THEN
                        REVOKE ALL ON TABLE operators_operator FROM jp_app;
                    END IF;
                    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'jp_admin') THEN
                        REVOKE ALL ON TABLE operators_operator FROM jp_admin;
                    END IF;
                END$$;
            """,
        ),
    ]
