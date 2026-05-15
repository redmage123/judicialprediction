"""
S6.15 — PersonalAccessToken table for the public REST API.

Each operator can mint multiple PATs; the plaintext is shown once at mint
time and only the SHA-256 hex hash is stored.  The Rust gateway resolves
``Authorization: Bearer pat_*`` headers against this table via
``rust/api-gateway/src/pat_auth.rs``.
"""

import uuid

from django.db import migrations, models


class Migration(migrations.Migration):
    dependencies = [
        ("operators", "0004_passwordresettoken"),
    ]

    operations = [
        migrations.CreateModel(
            name="PersonalAccessToken",
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
                ("name", models.CharField(max_length=255)),
                (
                    "token_hash",
                    models.CharField(
                        help_text="SHA-256 hex of the plaintext PAT (64 chars).",
                        max_length=64,
                        unique=True,
                    ),
                ),
                ("created_at", models.DateTimeField(auto_now_add=True)),
                ("last_used_at", models.DateTimeField(blank=True, null=True)),
                ("revoked_at", models.DateTimeField(blank=True, null=True)),
                ("expires_at", models.DateTimeField(blank=True, null=True)),
                (
                    "operator",
                    models.ForeignKey(
                        on_delete=models.deletion.CASCADE,
                        related_name="personal_access_tokens",
                        to="operators.operator",
                    ),
                ),
            ],
            options={
                "db_table": "personal_access_tokens",
            },
        ),
        migrations.AddIndex(
            model_name="personalaccesstoken",
            index=models.Index(
                fields=["operator"], name="personal_ac_operato_idx"
            ),
        ),
    ]
